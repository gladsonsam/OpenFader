//! Compact real-PCM extrema written by the capture disk worker, never by CPAL.
use crate::{invalid, TakeManifest, WaveformPreview, MAX_CHANNELS};
use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    time::{Duration, Instant},
};

const FILE: &str = "live-waveform.bin";
const MAGIC: &[u8; 8] = b"OFPEAK01";
const HEADER: u64 = 16;
pub(crate) fn discard(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(FILE));
}
fn bin_frames(rate: u32) -> u64 {
    u64::from(rate.div_ceil(50).max(1))
}

pub(crate) struct Writer {
    file: BufWriter<File>,
    channels: usize,
    bin_frames: u64,
    pending: u64,
    extrema: [[i32; 2]; MAX_CHANNELS],
    flushed: Instant,
}
impl Writer {
    pub(crate) fn create(dir: &Path, rate: u32, channels: usize) -> io::Result<Self> {
        let mut file = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(FILE))?,
        );
        file.write_all(MAGIC)?;
        file.write_all(&rate.to_le_bytes())?;
        file.write_all(&(channels as u16).to_le_bytes())?;
        file.write_all(&[0, 0])?;
        file.flush()?;
        Ok(Self {
            file,
            channels,
            bin_frames: bin_frames(rate),
            pending: 0,
            extrema: [[i32::MAX, i32::MIN]; MAX_CHANNELS],
            flushed: Instant::now(),
        })
    }
    pub(crate) fn append(&mut self, samples: &[i32]) -> io::Result<()> {
        for frame in samples.chunks_exact(self.channels) {
            for (ch, &v) in frame.iter().enumerate() {
                self.extrema[ch][0] = self.extrema[ch][0].min(v);
                self.extrema[ch][1] = self.extrema[ch][1].max(v);
            }
            self.pending += 1;
            if self.pending == self.bin_frames {
                for pair in &self.extrema[..self.channels] {
                    for &v in pair {
                        self.file
                            .write_all(&(v as f32 / 8_388_608.0).to_le_bytes())?;
                    }
                }
                self.pending = 0;
                self.extrema.fill([i32::MAX, i32::MIN]);
            }
        }
        if self.flushed.elapsed() >= Duration::from_millis(200) {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.flushed = Instant::now();
        Ok(())
    }
}

pub(crate) fn read(
    dir: &Path,
    manifest: &TakeManifest,
    start: u64,
    end: u64,
    buckets: usize,
) -> io::Result<WaveformPreview> {
    let mut file = BufReader::new(File::open(dir.join(FILE))?);
    let mut header = [0; HEADER as usize];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC
        || u32::from_le_bytes(header[8..12].try_into().unwrap()) != manifest.sample_rate
        || u16::from_le_bytes(header[12..14].try_into().unwrap()) != manifest.channels
    {
        return Err(invalid("live waveform layout does not match take"));
    }
    let channels = manifest.channels as usize;
    let stride = channels as u64 * 8;
    // Ignore an in-flight partial record. Only complete extrema bins are visible.
    let available = file.get_ref().metadata()?.len().saturating_sub(HEADER) / stride;
    let step = bin_frames(manifest.sample_rate);
    let first = (start / step).min(available);
    let last = (end / step).min(available).max(first);
    let count = buckets.min((last - first) as usize);
    let mut result = vec![vec![[1.0f32, -1.0f32]; count]; channels];
    file.seek(SeekFrom::Start(HEADER + first * stride))?;
    let mut record = vec![0; stride as usize];
    for n in first..last {
        file.read_exact(&mut record)?;
        let bucket = ((n - first) * count as u64 / (last - first)) as usize;
        for (ch, pair) in record.as_chunks::<8>().0.iter().enumerate() {
            let lo = f32::from_le_bytes(pair[..4].try_into().unwrap());
            let hi = f32::from_le_bytes(pair[4..].try_into().unwrap());
            if !lo.is_finite() || !hi.is_finite() || lo < -1.0 || hi > 1.0 || lo > hi {
                return Err(invalid("invalid live waveform extrema"));
            }
            result[ch][bucket][0] = result[ch][bucket][0].min(lo);
            result[ch][bucket][1] = result[ch][bucket][1].max(hi);
        }
    }
    Ok(WaveformPreview {
        start_frame: first * step,
        end_frame: last * step,
        buckets: count,
        complete: false,
        channels: result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{waveform_peaks, waveform_preview, ChannelLayout, TakeWriter};
    #[test]
    fn live_extrema_match_completed_pcm_for_different_layouts() {
        for channels in [1, 3, 64] {
            let dir = std::env::temp_dir().join(format!(
                "openfader-live-peaks-{}-{channels}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir(&dir).unwrap();
            let take_dir = dir.join("take");
            let mut take =
                TakeWriter::create(&take_dir, 100, 20, ChannelLayout::new(channels).unwrap())
                    .unwrap();
            let mut peaks = Writer::create(&take_dir, 100, channels as usize).unwrap();
            let samples: Vec<_> = (0..8)
                .flat_map(|f| (0..channels).map(move |ch| (f - 4) * 100_000 + i32::from(ch)))
                .collect();
            take.append_interleaved(&samples[..channels as usize * 4])
                .unwrap();
            peaks.append(&samples[..channels as usize * 4]).unwrap();
            peaks.flush().unwrap();
            let first = waveform_preview(&dir, "take", 0, 8, 4).unwrap();
            assert!(!first.complete);
            assert_eq!(first.end_frame, 4);
            assert_eq!(first.channels.len(), channels as usize);
            take.append_interleaved(&samples[channels as usize * 4..])
                .unwrap();
            peaks.append(&samples[channels as usize * 4..]).unwrap();
            peaks.flush().unwrap();
            let live = waveform_preview(&dir, "take", 0, 8, 4).unwrap();
            assert_eq!(live.end_frame, 8);
            drop(peaks);
            // A reader racing a partially flushed next bin must ignore it.
            OpenOptions::new()
                .append(true)
                .open(take_dir.join(FILE))
                .unwrap()
                .write_all(&[0; 4])
                .unwrap();
            assert_eq!(
                waveform_preview(&dir, "take", 0, 8, 4).unwrap().channels,
                live.channels
            );
            take.finish().unwrap();
            assert_eq!(
                live.channels,
                waveform_peaks(&dir, "take", 0, 8, 4).unwrap()
            );
            assert!(waveform_preview(&dir, "../take", 0, 8, 4).is_err());
            assert!(waveform_preview(&dir, "take", 0, 1_000_001, 4).is_err());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
