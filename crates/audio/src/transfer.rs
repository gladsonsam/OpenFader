//! Local filesystem transfer of complete takes. Callers must enforce access control.
use crate::{header, invalid, publish, read_manifest, ChannelLayout, TakeManifest, TakeWriter};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = if self.0.is_dir() {
            fs::remove_dir_all(&self.0)
        } else {
            fs::remove_file(&self.0)
        };
    }
}
fn stage(parent: &Path) -> io::Result<Stage> {
    loop {
        let path = parent.join(format!(
            ".openfader-transfer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(Stage(path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}
fn no_target(path: &Path) -> io::Result<()> {
    if path.symlink_metadata().is_ok() {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ))
    } else {
        Ok(())
    }
}
fn parent(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| invalid("destination needs a parent"))
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn commit_dir(stage: &Path, dest: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let src = CString::new(stage.as_os_str().as_bytes()).map_err(|_| invalid("invalid path"))?;
    let dst = CString::new(dest.as_os_str().as_bytes()).map_err(|_| invalid("invalid path"))?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            src.as_ptr(),
            libc::AT_FDCWD,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(src.as_ptr(), dst.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn commit_dir(stage: &Path, dest: &Path) -> io::Result<()> {
    no_target(dest)?;
    fs::rename(stage, dest)
}
fn commit_file(stage: &Path, dest: &Path) -> io::Result<()> {
    fs::hard_link(stage, dest)?;
    fs::remove_file(stage)
}

struct Wav {
    rate: u32,
    channels: u16,
    bits: u16,
    frames: u64,
    offset: u64,
    names: Vec<String>,
}
fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn parse(f: &mut File) -> io::Result<Wav> {
    let len = f.metadata()?.len();
    let mut head = [0; 12];
    f.read_exact(&mut head)?;
    if &head[..4] != b"RIFF" || &head[8..] != b"WAVE" || u64::from(u32le(&head[4..8])) + 8 != len {
        return Err(invalid("invalid or incomplete RIFF/WAVE"));
    }
    let mut format = None;
    let mut data = None;
    let mut names = Vec::new();
    while f.stream_position()? < len {
        let mut ch = [0; 8];
        f.read_exact(&mut ch)?;
        let size = u64::from(u32le(&ch[4..]));
        let start = f.stream_position()?;
        let end = start
            .checked_add(size)
            .and_then(|x| {
                if x == len {
                    Some(x)
                } else {
                    x.checked_add(size % 2)
                }
            })
            .filter(|&x| x <= len)
            .ok_or_else(|| invalid("invalid WAV chunk length"))?;
        match &ch[..4] {
            b"fmt " => {
                if format.is_some() || !(16..=4096).contains(&size) {
                    return Err(invalid("invalid WAV format chunk"));
                }
                let mut b = vec![0; size as usize];
                f.read_exact(&mut b)?;
                let tag = u16le(&b);
                let channels = u16le(&b[2..]);
                let rate = u32le(&b[4..]);
                let align = u16le(&b[12..]);
                let bits = u16le(&b[14..]);
                let pcm = tag == 1
                    || (tag == 0xfffe
                        && size >= 40
                        && u16le(&b[16..]) >= 22
                        && b[24..40] == [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
                        && u16le(&b[18..]) == bits);
                ChannelLayout::new(channels)?;
                if !pcm
                    || ![16, 24, 32].contains(&bits)
                    || rate == 0
                    || align != channels * (bits / 8)
                    || rate.checked_mul(u32::from(align)) != Some(u32le(&b[8..]))
                {
                    return Err(invalid("unsupported WAV PCM format"));
                }
                format = Some((rate, channels, bits, align));
            }
            b"data" => {
                if data.replace((start, size)).is_some() {
                    return Err(invalid("duplicate WAV data"));
                }
            }
            b"ofnm" => {
                if size > 65536 || !names.is_empty() {
                    return Err(invalid("invalid channel names"));
                }
                let mut b = vec![0; size as usize];
                f.read_exact(&mut b)?;
                names = serde_json::from_slice(&b).map_err(|_| invalid("invalid channel names"))?;
            }
            _ => {}
        }
        f.seek(SeekFrom::Start(end))?;
    }
    let (rate, channels, bits, align) = format.ok_or_else(|| invalid("missing WAV format"))?;
    let (offset, size) = data.ok_or_else(|| invalid("missing WAV data"))?;
    if size == 0
        || size % u64::from(align) != 0
        || (!names.is_empty() && names.len() != channels as usize)
    {
        return Err(invalid("invalid WAV frames or channel names"));
    }
    Ok(Wav {
        rate,
        channels,
        bits,
        frames: size / u64::from(align),
        offset,
        names,
    })
}
fn validate_take(dir: &Path) -> io::Result<TakeManifest> {
    let m = read_manifest(dir)?;
    if !m.complete || m.frames == 0 {
        return Err(invalid("take incomplete or empty"));
    }
    for s in &m.segments {
        let mut f = File::open(dir.join(&s.file))?;
        let w = parse(&mut f)?;
        if w.rate != m.sample_rate
            || w.channels != m.channels
            || w.bits != 24
            || w.frames != s.frames
            || w.offset != 44
        {
            return Err(invalid("segment does not match manifest"));
        }
    }
    Ok(m)
}
/// Import 16/24/32-bit integer PCM WAV into a new native take directory.
pub fn import_wav(source: &Path, destination: &Path) -> io::Result<TakeManifest> {
    no_target(destination)?;
    let mut input = File::open(source)?;
    let wav = parse(&mut input)?;
    input.seek(SeekFrom::Start(wav.offset))?;
    let tmp = stage(parent(destination)?)?;
    let staged = tmp.0.join("content");
    let layout = ChannelLayout::new(wav.channels)?;
    let mut writer = TakeWriter::create_with_layout(
        &staged,
        wav.rate,
        (u32::MAX as u64 - 36) / layout.frame_bytes(),
        layout,
    )?;
    let bytes = usize::from(wav.bits / 8);
    let samples_per_block = usize::from(wav.channels) * 1024;
    let mut raw = vec![0u8; samples_per_block * bytes];
    let mut left = wav.frames;
    while left > 0 {
        let frames = left.min(1024) as usize;
        let n = frames * usize::from(wav.channels) * bytes;
        input.read_exact(&mut raw[..n])?;
        let samples: Vec<i32> = raw[..n]
            .chunks_exact(bytes)
            .map(|b| match bytes {
                2 => i32::from(i16::from_le_bytes([b[0], b[1]])) << 8,
                3 => i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8,
                _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 8,
            })
            .collect();
        writer.append_interleaved(&samples)?;
        left -= frames as u64;
    }
    let mut m = writer.finish()?;
    m.channel_names = wav.names;
    publish(&staged, &m)?;
    commit_dir(&staged, destination)?;
    Ok(m)
}
/// Copy a validated native take to a new directory.
pub fn copy_take(source: &Path, destination: &Path) -> io::Result<TakeManifest> {
    no_target(destination)?;
    let m = validate_take(source)?;
    let tmp = stage(parent(destination)?)?;
    let staged = tmp.0.join("content");
    fs::create_dir(&staged)?;
    for s in &m.segments {
        fs::copy(source.join(&s.file), staged.join(&s.file))?;
    }
    publish(&staged, &m)?;
    validate_take(&staged)?;
    commit_dir(&staged, destination)?;
    Ok(m)
}
/// Export a complete take as 24-bit interleaved PCM WAV, preserving names in an optional OpenFader chunk.
pub fn export_wav(source: &Path, destination: &Path) -> io::Result<TakeManifest> {
    no_target(destination)?;
    let m = validate_take(source)?;
    let layout = ChannelLayout::new(m.channels)?;
    let data = m
        .frames
        .checked_mul(layout.frame_bytes())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| invalid("take exceeds WAV size limit"))?;
    let names = if m.channel_names.is_empty() {
        Vec::new()
    } else {
        serde_json::to_vec(&m.channel_names).map_err(io::Error::other)?
    };
    let extra = if names.is_empty() {
        0
    } else {
        8 + names.len() + (names.len() % 2)
    };
    let pad = (data % 2) as usize;
    let riff = 36u64 + u64::from(data) + extra as u64 + pad as u64;
    if riff > u64::from(u32::MAX) {
        return Err(invalid("take exceeds WAV size limit"));
    }
    let tmp = stage(parent(destination)?)?;
    let staged = tmp.0.join("content");
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
    header(&mut out, m.sample_rate, m.frames, layout)?;
    out.seek(SeekFrom::End(0))?;
    for s in &m.segments {
        let mut f = File::open(source.join(&s.file))?;
        f.seek(SeekFrom::Start(44))?;
        io::copy(&mut f.take(s.frames * layout.frame_bytes()), &mut out)?;
    }
    if pad != 0 {
        out.write_all(&[0])?;
    }
    if !names.is_empty() {
        out.write_all(b"ofnm")?;
        out.write_all(&(names.len() as u32).to_le_bytes())?;
        out.write_all(&names)?;
        if names.len() % 2 != 0 {
            out.write_all(&[0])?;
        }
    }
    out.seek(SeekFrom::Start(4))?;
    out.write_all(&(riff as u32).to_le_bytes())?;
    out.sync_all()?;
    drop(out);
    commit_file(&staged, destination)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "openfader-transfer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn wav_and_native_roundtrip_preserve_pcm_and_names() {
        let root = fixture();
        let source = root.join("source");
        let mut w =
            TakeWriter::create_with_layout(&source, 44100, 2, ChannelLayout::new(3).unwrap())
                .unwrap();
        w.append_interleaved(&[1, -2, 8388607, -8388608, 500, -100, 42, 43, 44])
            .unwrap();
        let mut m = w.finish().unwrap();
        m.channel_names = vec!["Kick".into(), "Vox".into(), "Synth".into()];
        publish(&source, &m).unwrap();
        let wav = root.join("out.wav");
        export_wav(&source, &wav).unwrap();
        let imported = root.join("imported");
        assert_eq!(
            import_wav(&wav, &imported).unwrap().channel_names,
            m.channel_names
        );
        let mut original = Vec::new();
        for s in &m.segments {
            let mut b = fs::read(source.join(&s.file)).unwrap();
            original.extend(b.drain(44..));
        }
        let im = read_manifest(&imported).unwrap();
        let mut actual = Vec::new();
        for s in &im.segments {
            let mut b = fs::read(imported.join(&s.file)).unwrap();
            actual.extend(b.drain(44..));
        }
        assert_eq!(actual, original);
        assert_eq!(im.frames, 3);
        assert_eq!(im.sample_rate, 44100);
        assert_eq!(copy_take(&source, &root.join("copy")).unwrap(), m);
        assert!(export_wav(&source, &wav).is_err());
        assert!(import_wav(&wav, &imported).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_truncation_incomplete_and_bad_format_without_leaving_destination() {
        let root = fixture();
        let source = root.join("source");
        let mut w =
            TakeWriter::create_with_layout(&source, 48000, 10, ChannelLayout::new(1).unwrap())
                .unwrap();
        w.append_interleaved(&[1, 2]).unwrap();
        w.finish().unwrap();
        let wav = root.join("out.wav");
        export_wav(&source, &wav).unwrap();
        let mut bytes = fs::read(&wav).unwrap();
        bytes.pop();
        fs::write(&wav, &bytes).unwrap();
        assert!(import_wav(&wav, &root.join("bad")).is_err());
        assert!(!root.join("bad").exists());
        let mut m = read_manifest(&source).unwrap();
        m.complete = false;
        publish(&source, &m).unwrap();
        assert!(copy_take(&source, &root.join("bad-native")).is_err());
        assert!(!root.join("bad-native").exists());
        assert!(!fs::read_dir(&root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".openfader-transfer-")));
        fs::remove_dir_all(root).unwrap();
    }
}
