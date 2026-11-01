//! Durable multichannel take storage and desktop recorder service.

pub mod process_supervisor;
pub mod transfer;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub input_channels: u16,
    pub output_channels: u16,
    pub sample_rate: u32,
}

pub const CHANNELS: usize = 18;
/// Normalize an optional output mask against the selected device layout.
pub fn playback_output_mask(mask: Option<Vec<bool>>, channels: u16) -> std::io::Result<Vec<bool>> {
    ChannelLayout::new(channels)?;
    let mask = mask.unwrap_or_default();
    if mask.is_empty() {
        return Ok(vec![true; usize::from(channels)]);
    }
    if mask.len() != usize::from(channels) {
        return Err(std::io::Error::other(
            "output mask length differs from device outputs",
        ));
    }
    Ok(mask)
}

/// Hold while a playback helper can open or emit audio. A recovering host must
/// acquire this same per-recorder lock before treating old output as stopped.
pub fn playback_guard(root: &std::path::Path) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("playback.lock"))?;
    file.try_lock().map_err(std::io::Error::other)?;
    Ok(file)
}

#[cfg(test)]
mod mask_defaults_tests {
    use super::playback_output_mask;
    #[test]
    fn masks_follow_actual_output_layout() {
        for channels in [16, 18, 32] {
            let expected = vec![true; usize::from(channels)];
            assert_eq!(playback_output_mask(None, channels).unwrap(), expected);
            assert_eq!(
                playback_output_mask(Some(vec![]), channels).unwrap(),
                expected
            );
            let supplied = vec![false; usize::from(channels)];
            assert_eq!(
                playback_output_mask(Some(supplied.clone()), channels).unwrap(),
                supplied
            );
            assert!(playback_output_mask(Some(vec![true; 3]), channels).is_err());
        }
        assert!(playback_output_mask(None, 0).is_err());
    }
}
#[cfg(test)]
const FRAME_BYTES: u64 = CHANNELS as u64 * 3;
pub const MAX_CHANNELS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelLayout(u16);
impl ChannelLayout {
    pub fn new(channels: u16) -> io::Result<Self> {
        if channels == 0 || channels as usize > MAX_CHANNELS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid channel count",
            ));
        }
        Ok(Self(channels))
    }
    pub fn channels(self) -> u16 {
        self.0
    }
    fn frame_bytes(self) -> u64 {
        u64::from(self.0) * 3
    }
}
const HEADER_BYTES: u64 = 44;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Segment {
    pub file: String,
    pub start_frame: u64,
    pub frames: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TakeManifest {
    pub version: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub complete: bool,
    pub segments: Vec<Segment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_names: Vec<String>,
    #[serde(flatten)]
    pub metadata: std::collections::BTreeMap<String, serde_json::Value>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn header(file: &mut File, rate: u32, frames: u64, layout: ChannelLayout) -> io::Result<()> {
    let size = frames
        .checked_mul(layout.frame_bytes())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| invalid("WAV segment too large"))?;
    let riff = size
        .checked_add(36)
        .ok_or_else(|| invalid("WAV segment too large"))?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(b"RIFF")?;
    file.write_all(&riff.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&layout.channels().to_le_bytes())?;
    file.write_all(&rate.to_le_bytes())?;
    let byte_rate = rate
        .checked_mul(layout.frame_bytes() as u32)
        .ok_or_else(|| invalid("sample rate too large"))?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(layout.frame_bytes() as u16).to_le_bytes())?;
    file.write_all(&24u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&size.to_le_bytes())?;
    Ok(())
}
fn publish(dir: &Path, manifest: &TakeManifest) -> io::Result<()> {
    let tmp = dir.join("manifest.tmp");
    let mut f = File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut f, manifest).map_err(io::Error::other)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    fs::rename(tmp, dir.join("manifest.json"))?;
    // Directory sync is available on Unix; Windows does not permit opening a directory as a File.
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    Ok(())
}
pub fn read_manifest(dir: impl AsRef<Path>) -> io::Result<TakeManifest> {
    let manifest: TakeManifest =
        serde_json::from_reader(File::open(dir.as_ref().join("manifest.json"))?)
            .map_err(io::Error::other)?;
    validate(&manifest)?;
    Ok(manifest)
}
fn validate(m: &TakeManifest) -> io::Result<()> {
    if m.version != 1 || ChannelLayout::new(m.channels).is_err() || m.sample_rate == 0 {
        return Err(invalid("unsupported take manifest"));
    }
    if !m.channel_names.is_empty() && m.channel_names.len() != usize::from(m.channels) {
        return Err(invalid("invalid channel names"));
    }
    let mut frame = 0u64;
    for (i, s) in m.segments.iter().enumerate() {
        if s.file != format!("segment-{i:06}.wav")
            || s.start_frame != frame
            || s.frames == 0
            || s.frames > (u32::MAX as u64 - 36) / (u64::from(m.channels) * 3)
        {
            return Err(invalid("invalid segment"));
        }
        frame = frame
            .checked_add(s.frames)
            .ok_or_else(|| invalid("frame overflow"))?;
    }
    if frame != m.frames {
        return Err(invalid("invalid frame count"));
    }
    Ok(())
}
pub struct TakeWriter {
    dir: PathBuf,
    manifest: TakeManifest,
    limit: u64,
    file: Option<File>,
}
impl TakeWriter {
    pub fn create(
        dir: impl AsRef<Path>,
        sample_rate: u32,
        segment_frames: u64,
    ) -> io::Result<Self> {
        Self::create_with_layout(
            dir,
            sample_rate,
            segment_frames,
            ChannelLayout(CHANNELS as u16),
        )
    }
    pub fn create_with_layout(
        dir: impl AsRef<Path>,
        sample_rate: u32,
        segment_frames: u64,
        layout: ChannelLayout,
    ) -> io::Result<Self> {
        if sample_rate == 0
            || sample_rate
                .checked_mul(layout.frame_bytes() as u32)
                .is_none()
            || segment_frames == 0
            || segment_frames > (u32::MAX as u64 - 36) / layout.frame_bytes()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid rate or segment size",
            ));
        }
        let dir = dir.as_ref().to_owned();
        fs::create_dir(&dir)?;
        let manifest = TakeManifest {
            version: 1,
            sample_rate,
            channels: layout.channels(),
            frames: 0,
            complete: false,
            segments: vec![],
            channel_names: vec![],
            metadata: Default::default(),
        };
        publish(&dir, &manifest)?;
        Ok(Self {
            dir,
            manifest,
            limit: segment_frames,
            file: None,
        })
    }
    pub fn append_interleaved(&mut self, samples: &[i32]) -> io::Result<()> {
        let channels = self.manifest.channels as usize;
        if !samples.len().is_multiple_of(channels)
            || samples
                .iter()
                .any(|&v| !(-8_388_608..=8_388_607).contains(&v))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected complete 24-bit frames",
            ));
        }
        for frame in samples.chunks_exact(channels) {
            if self.file.is_none() {
                let name = format!("segment-{:06}.wav", self.manifest.segments.len());
                let mut f = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.dir.join(&name))?;
                header(
                    &mut f,
                    self.manifest.sample_rate,
                    0,
                    ChannelLayout(self.manifest.channels),
                )?;
                self.manifest.segments.push(Segment {
                    file: name,
                    start_frame: self.manifest.frames,
                    frames: 0,
                });
                self.file = Some(f);
            }
            let f = self.file.as_mut().unwrap();
            let mut bytes = [0u8; MAX_CHANNELS * 3];
            for (sample, out) in frame.iter().zip(bytes.as_chunks_mut::<3>().0.iter_mut()) {
                out.copy_from_slice(&sample.to_le_bytes()[..3]);
            }
            f.write_all(&bytes[..channels * 3])?;
            let seg = self.manifest.segments.last_mut().unwrap();
            seg.frames += 1;
            self.manifest.frames += 1;
            if seg.frames == self.limit {
                self.checkpoint()?;
                self.file = None;
            }
        }
        Ok(())
    }
    /// Flush and atomically publish the current complete frames.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        if let Some(f) = &mut self.file {
            let frames = self.manifest.segments.last().unwrap().frames;
            header(
                f,
                self.manifest.sample_rate,
                frames,
                ChannelLayout(self.manifest.channels),
            )?;
            f.sync_all()?;
            f.seek(SeekFrom::End(0))?;
        }
        publish(&self.dir, &self.manifest)
    }
    pub fn finish(mut self) -> io::Result<TakeManifest> {
        self.checkpoint()?;
        self.manifest.complete = true;
        publish(&self.dir, &self.manifest)?;
        Ok(self.manifest)
    }
}
/// Restore only bytes committed in the manifest; never invent missing samples.
pub fn recover(dir: impl AsRef<Path>) -> io::Result<TakeManifest> {
    let dir = dir.as_ref();
    let mut m = read_manifest(dir)?;
    if m.complete {
        return Ok(m);
    }
    for s in &m.segments {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.join(&s.file))?;
        let expected = HEADER_BYTES + s.frames * u64::from(m.channels) * 3;
        if f.metadata()?.len() < expected {
            return Err(invalid("segment shorter than checkpoint"));
        }
        let mut magic = [0; 4];
        f.read_exact(&mut magic)?;
        if &magic != b"RIFF" {
            return Err(invalid("invalid WAV"));
        }
        f.set_len(expected)?;
        header(&mut f, m.sample_rate, s.frames, ChannelLayout(m.channels))?;
        f.sync_all()?;
    }
    // Remove only segment files generated by this crate beyond the checkpoint.
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(n) = name
            .strip_prefix("segment-")
            .and_then(|x| x.strip_suffix(".wav"))
        {
            if n.len() == 6
                && n.bytes().all(|b| b.is_ascii_digit())
                && n.parse::<usize>().is_ok_and(|i| i >= m.segments.len())
            {
                fs::remove_file(entry.path())?;
            }
        }
    }
    m.complete = false;
    publish(dir, &m)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn path() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "openfader-audio-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn variable_layout_storage_and_recovery() {
        for channels in [16u16, 18, 32] {
            let root = path();
            fs::create_dir(&root).unwrap();
            let dir = root.join("take");
            let layout = ChannelLayout::new(channels).unwrap();
            let mut w = TakeWriter::create_with_layout(&dir, 48_000, 3, layout).unwrap();
            assert!(w
                .append_interleaved(&vec![0; channels as usize - 1])
                .is_err());
            w.append_interleaved(&vec![123; channels as usize * 2])
                .unwrap();
            w.checkpoint().unwrap();
            drop(w);
            assert_eq!(recover(&dir).unwrap().frames, 2);
            let mut manifest = read_manifest(&dir).unwrap();
            manifest.complete = true;
            publish(&dir, &manifest).unwrap();
            assert_eq!(
                inspect_take(&root, "take").unwrap()["channels"]
                    .as_array()
                    .unwrap()
                    .len(),
                channels as usize
            );
            assert_eq!(
                waveform_peaks(&root, "take", 0, 2, 1).unwrap().len(),
                channels as usize
            );
            fs::remove_dir_all(root).unwrap();
        }
        assert!(ChannelLayout::new(0).is_err());
        assert!(ChannelLayout::new(65).is_err());
    }
    #[test]
    fn segments_keep_channel_frames_together() {
        let dir = path();
        let mut w = TakeWriter::create(&dir, 48_000, 2).unwrap();
        let samples: Vec<i32> = (0..CHANNELS * 5).map(|i| i as i32 - 45).collect();
        w.append_interleaved(&samples).unwrap();
        let m = w.finish().unwrap();
        assert_eq!(
            m.segments
                .iter()
                .map(|s| (s.start_frame, s.frames))
                .collect::<Vec<_>>(),
            vec![(0, 2), (2, 2), (4, 1)]
        );
        let data = fs::read(dir.join(&m.segments[0].file)).unwrap();
        assert_eq!(data.len(), 44 + 2 * CHANNELS * 3);
        assert_eq!(&data[44..47], &(-45i32).to_le_bytes()[..3]);
        assert_eq!(read_manifest(&dir).unwrap(), m);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recovery_discards_uncheckpointed_frames_and_segments() {
        let dir = path();
        let mut w = TakeWriter::create(&dir, 48_000, 3).unwrap();
        w.append_interleaved(&[1; CHANNELS * 2]).unwrap();
        w.checkpoint().unwrap();
        w.append_interleaved(&[2; CHANNELS * 2]).unwrap();
        drop(w);
        let m = recover(&dir).unwrap();
        // Filling the first segment automatically checkpoints three frames.
        assert_eq!(m.frames, 3);
        assert!(!m.complete);
        assert_eq!(
            fs::metadata(dir.join("segment-000000.wav")).unwrap().len(),
            44 + 3 * FRAME_BYTES
        );
        assert!(!dir.join("segment-000001.wav").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rejects_incomplete_or_out_of_range_frames() {
        let dir = path();
        let mut w = TakeWriter::create(&dir, 48_000, 5).unwrap();
        assert!(w.append_interleaved(&[0]).is_err());
        assert!(w.append_interleaved(&[8_388_608; CHANNELS]).is_err());
        assert_eq!(w.finish().unwrap().frames, 0);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(target_os = "macos")]
mod macos_devices;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod service;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use service::{RecorderService, RecorderStatus};

/// Read a bounded interval of a completed take and aggregate real PCM extrema.
pub fn waveform_peaks(
    root: &Path,
    id: &str,
    start: u64,
    end: u64,
    buckets: usize,
) -> io::Result<Vec<Vec<[f32; 2]>>> {
    use std::io::BufReader;
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(invalid("invalid take ID"));
    }
    let length = end
        .checked_sub(start)
        .ok_or_else(|| invalid("invalid range"))?;
    if length == 0
        || length > 1_000_000
        || buckets == 0
        || buckets > 2048
        || buckets as u64 > length
    {
        return Err(invalid("invalid waveform range or resolution"));
    }
    let dir = root.join(id);
    let manifest = read_manifest(&dir)?;
    if !manifest.complete || end > manifest.frames {
        return Err(invalid("take incomplete or waveform range exceeds take"));
    }
    let channels = manifest.channels as usize;
    let frame_bytes = channels as u64 * 3;
    let mut result = vec![vec![[1.0f32, -1.0f32]; buckets]; channels];
    for segment in &manifest.segments {
        let lo = start.max(segment.start_frame);
        let hi = end.min(segment.start_frame + segment.frames);
        if lo >= hi {
            continue;
        }
        let mut file = BufReader::new(File::open(dir.join(&segment.file))?);
        file.seek(SeekFrom::Start(
            HEADER_BYTES + (lo - segment.start_frame) * frame_bytes,
        ))?;
        let mut bytes = vec![0u8; channels * 3];
        for frame in lo..hi {
            file.read_exact(&mut bytes)?;
            // The inverse of floor(bucket * length / buckets).
            let bucket = (((frame - start + 1) * buckets as u64 - 1) / length) as usize;
            for (channel, sample) in bytes.as_chunks::<3>().0.iter().enumerate() {
                let value = i32::from_le_bytes([
                    sample[0],
                    sample[1],
                    sample[2],
                    if sample[2] & 0x80 != 0 { 0xff } else { 0 },
                ]) as f32
                    / 8_388_608.0;
                result[channel][bucket][0] = result[channel][bucket][0].min(value);
                result[channel][bucket][1] = result[channel][bucket][1].max(value);
            }
        }
    }
    Ok(result)
}

/// Inspect WAV headers and a bounded initial interval without exposing samples.
pub fn inspect_take(root: &Path, id: &str) -> io::Result<serde_json::Value> {
    use std::io::BufReader;
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(invalid("invalid take ID"));
    }
    let dir = root.join(id);
    let manifest = read_manifest(&dir)?;
    if !manifest.complete || manifest.frames == 0 {
        return Err(invalid("take incomplete or empty"));
    }
    let end = manifest.frames.min(1_000_000);
    let mut headers = Vec::new();
    let channels = manifest.channels as usize;
    let frame_bytes = channels as u64 * 3;
    let mut min = vec![1.0f64; channels];
    let mut max = vec![-1.0f64; channels];
    let mut squares = vec![0.0f64; channels];
    for segment in &manifest.segments {
        let mut f = BufReader::new(File::open(dir.join(&segment.file))?);
        let mut h = [0u8; 44];
        f.read_exact(&mut h)?;
        if &h[0..4] != b"RIFF" || &h[8..16] != b"WAVEfmt " || &h[36..40] != b"data" {
            return Err(invalid("unsupported WAV header"));
        }
        let channels = u16::from_le_bytes([h[22], h[23]]);
        let bits = u16::from_le_bytes([h[34], h[35]]);
        let size = u32::from_le_bytes(h[40..44].try_into().unwrap()) as u64;
        let rate = u32::from_le_bytes(h[24..28].try_into().unwrap());
        if u16::from_le_bytes([h[20], h[21]]) != 1
            || channels != manifest.channels
            || bits != 24
            || rate != manifest.sample_rate
            || !size.is_multiple_of(frame_bytes)
            || size / frame_bytes != segment.frames
            || u16::from_le_bytes([h[32], h[33]]) as u64 != frame_bytes
        {
            return Err(invalid("unsupported WAV format"));
        }
        headers.push(serde_json::json!({"file":segment.file,"channels":channels,"bits_per_sample":bits,"sample_rate":rate,"frames":size / frame_bytes}));
        let count = end.saturating_sub(segment.start_frame).min(segment.frames);
        let mut bytes = vec![0u8; manifest.channels as usize * 3];
        for _ in 0..count {
            f.read_exact(&mut bytes)?;
            for (ch, sample) in bytes.as_chunks::<3>().0.iter().enumerate() {
                let value = i32::from_le_bytes([
                    sample[0],
                    sample[1],
                    sample[2],
                    if sample[2] & 0x80 != 0 { 255 } else { 0 },
                ]) as f64
                    / 8_388_608.0;
                min[ch] = min[ch].min(value);
                max[ch] = max[ch].max(value);
                squares[ch] += value * value;
            }
        }
    }
    let channels: Vec<_> = (0..channels).map(|i| serde_json::json!({"channel":i+1,"min":min[i],"max":max[i],"rms":(squares[i] / end as f64).sqrt()})).collect();
    Ok(
        serde_json::json!({"manifest":manifest,"wav_headers":headers,"start_frame":0,"end_frame":end,"channels":channels}),
    )
}

#[cfg(test)]
mod waveform_tests {
    use super::*;
    #[test]
    fn extrema_follow_channels_and_segment_boundaries() {
        let root = std::env::temp_dir().join(format!(
            "openfader-peaks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let mut writer = TakeWriter::create(root.join("take"), 48000, 2).unwrap();
        let mut data = vec![0; CHANNELS * 4];
        for (frame, sample) in [-8_388_608, 4_194_304, -4_194_304, 8_388_607]
            .into_iter()
            .enumerate()
        {
            data[frame * CHANNELS] = sample;
        }
        writer.append_interleaved(&data).unwrap();
        writer.finish().unwrap();
        let peaks = waveform_peaks(&root, "take", 0, 4, 2).unwrap();
        assert_eq!(peaks[0][0], [-1.0, 0.5]);
        assert_eq!(peaks[0][1], [-0.5, 8_388_607.0 / 8_388_608.0]);
        assert_eq!(peaks[1], vec![[0.0, 0.0]; 2]);
        assert!(waveform_peaks(&root, "../take", 0, 4, 2).is_err());
        assert!(waveform_peaks(&root, "take", 0, 5, 2).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
