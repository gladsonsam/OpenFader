use crate::{
    read_manifest, recover, ChannelLayout, DeviceInfo, TakeManifest, TakeWriter, MAX_CHANNELS,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

// Select the device's native PCM representation; ASIO drivers often expose
// integer samples only. The realtime callbacks convert without allocating.
macro_rules! pcm_stream {
    ($format:expr, $build:ident) => {
        match $format {
            cpal::SampleFormat::I8 => $build!(i8),
            cpal::SampleFormat::I16 => $build!(i16),
            cpal::SampleFormat::I24 => $build!(cpal::I24),
            cpal::SampleFormat::I32 => $build!(i32),
            cpal::SampleFormat::I64 => $build!(i64),
            cpal::SampleFormat::U8 => $build!(u8),
            cpal::SampleFormat::U16 => $build!(u16),
            cpal::SampleFormat::U24 => $build!(cpal::U24),
            cpal::SampleFormat::U32 => $build!(u32),
            cpal::SampleFormat::U64 => $build!(u64),
            cpal::SampleFormat::F32 => $build!(f32),
            cpal::SampleFormat::F64 => $build!(f64),
            _ => return Err(error("device uses an unsupported non-PCM sample format")),
        }
    };
}
fn capture_sample<T: cpal::Sample>(sample: T) -> i32
where
    f64: cpal::FromSample<T>,
{
    let value = sample.to_sample::<f64>();
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(-1.0, 1.0) * 8_388_608.0)
        .round()
        .clamp(-8_388_608.0, 8_388_607.0) as i32
}

fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
fn valid_id(id: &str) -> io::Result<()> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid take id",
        ));
    }
    Ok(())
}
#[cfg(any(target_os = "macos", test))]
fn validate_playback_config(channels: u16, rate: u32, take_rate: u32) -> io::Result<()> {
    ChannelLayout::new(channels)?;
    if rate != take_rate {
        return Err(error("device sample rate differs from take"));
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecorderStatus {
    pub selected_device_id: Option<String>,
    pub selected_take_id: Option<String>,
    pub transport: String,
    pub sample_rate: Option<u32>,
    pub recorded_frames: u64,
    pub position_frames: u64,
    pub xruns: u64,
    pub live_peaks: Option<Vec<f32>>,
    pub error: Option<String>,
}
struct PreparedCapture {
    stream: cpal::Stream,
    consumer: rtrb::Consumer<[i32; MAX_CHANNELS]>,
    done: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    xruns: Arc<AtomicU64>,
    frames: Arc<AtomicU64>,
    peaks: Arc<[AtomicU32; MAX_CHANNELS]>,
    rate: u32,
    channels: usize,
}
struct Capture {
    stream: cpal::Stream,
    done: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    xruns: Arc<AtomicU64>,
    frames: Arc<AtomicU64>,
    peaks: Arc<[AtomicU32; MAX_CHANNELS]>,
    channels: usize,
    worker: JoinHandle<io::Result<TakeManifest>>,
}
struct Playback {
    stream: cpal::Stream,
    done: Arc<AtomicBool>,
    fade: Arc<AtomicBool>,
    faded: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
    xruns: Arc<AtomicU64>,
    worker: JoinHandle<io::Result<()>>,
}
type PlaybackFrame = Option<(u64, [f32; MAX_CHANNELS])>;

struct PlaybackOutput {
    consumer: rtrb::Consumer<PlaybackFrame>,
    done: Arc<AtomicBool>,
    fade: Arc<AtomicBool>,
    faded: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
    xruns: Arc<AtomicU64>,
    mask: Vec<bool>,
    take_channels: usize,
    mapping: Option<Vec<Option<usize>>>,
    ramp: f32,
    underrun: bool,
}
impl PlaybackOutput {
    fn render<T: cpal::SizedSample + cpal::FromSample<f32>>(&mut self, output: &mut [T]) {
        // Clear any incomplete frame as well as every frame after EOF/failure.
        output.fill(T::EQUILIBRIUM);
        for out in output.chunks_exact_mut(self.mask.len()) {
            if self.done.load(Ordering::Acquire)
                || self.ended.load(Ordering::Acquire)
                || self.faded.load(Ordering::Acquire)
            {
                self.faded.store(true, Ordering::Release);
                continue;
            }
            match self.consumer.pop() {
                Ok(None) => {
                    self.ended.store(true, Ordering::Release);
                    self.faded.store(true, Ordering::Release);
                }
                Ok(Some((frame, samples))) => {
                    self.underrun = false;
                    if self.fade.load(Ordering::Acquire) {
                        self.ramp = (self.ramp - 1.0 / 256.0).max(0.0);
                        if self.ramp == 0.0 {
                            self.faded.store(true, Ordering::Release);
                        }
                    } else {
                        self.ramp = (self.ramp + 1.0 / 256.0).min(1.0);
                    }
                    for (i, o) in out.iter_mut().enumerate() {
                        let track = self.mapping.as_ref().and_then(|m| m[i]).or_else(|| {
                            if self.mapping.is_none() && i < self.take_channels && self.mask[i] {
                                Some(i)
                            } else {
                                None
                            }
                        });
                        if let Some(track) = track {
                            let value = samples[track] * self.ramp;
                            *o = T::from_sample(
                                if matches!(
                                    T::FORMAT,
                                    cpal::SampleFormat::F32 | cpal::SampleFormat::F64
                                ) {
                                    value
                                } else {
                                    value.clamp(-1.0, 1.0 - f32::EPSILON)
                                },
                            );
                        }
                    }
                    self.position.store(frame + 1, Ordering::Release);
                }
                Err(_) => {
                    self.ramp = 0.0;
                    if !self.underrun {
                        self.xruns.fetch_add(1, Ordering::Relaxed);
                        self.underrun = true;
                    }
                    if self.fade.load(Ordering::Acquire) {
                        self.faded.store(true, Ordering::Release);
                    }
                }
            }
        }
    }
}

fn playback_worker(
    mut producer: rtrb::Producer<PlaybackFrame>,
    dir: PathBuf,
    manifest: TakeManifest,
    mut at: u64,
    looping: Option<(u64, u64)>,
    done: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut buffered = 0usize;
    let result = (|| -> io::Result<()> {
        let mut open: Option<(usize, fs::File)> = None;
        while !done.load(Ordering::Acquire) {
            if let Some((start, end)) = looping {
                if at >= end {
                    at = start;
                }
            }
            if at >= manifest.frames {
                if producer.push(None).is_ok() {
                    ready.store(true, Ordering::Release);
                    break;
                }
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            if producer.is_full() {
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            let idx = manifest
                .segments
                .iter()
                .position(|s| at >= s.start_frame && at < s.start_frame + s.frames)
                .ok_or_else(|| error("missing segment"))?;
            if open.as_ref().is_none_or(|(i, _)| *i != idx) {
                open = Some((idx, fs::File::open(dir.join(&manifest.segments[idx].file))?));
            }
            let f = &mut open.as_mut().unwrap().1;
            f.seek(SeekFrom::Start(
                44 + (at - manifest.segments[idx].start_frame) * manifest.channels as u64 * 3,
            ))?;
            let mut bytes = [0u8; MAX_CHANNELS * 3];
            f.read_exact(&mut bytes[..manifest.channels as usize * 3])?;
            let mut samples = [0f32; MAX_CHANNELS];
            for (sample, b) in samples.iter_mut().zip(bytes.as_chunks::<3>().0.iter()) {
                let value =
                    i32::from_le_bytes([b[0], b[1], b[2], if b[2] & 128 != 0 { 255 } else { 0 }]);
                *sample = value as f32 / 8_388_608.0;
            }
            if producer.push(Some((at, samples))).is_ok() {
                buffered += 1;
                if buffered >= 1024 {
                    ready.store(true, Ordering::Release);
                }
                at += 1;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        done.store(true, Ordering::Release);
    }
    result
}

pub struct RecorderService {
    root: PathBuf,
    selected: Option<String>,
    take: Option<String>,
    capture: Option<Capture>,
    prepared: Option<PreparedCapture>,
    last_error: Option<String>,
    last_frames: u64,
    last_xruns: u64,
    rate: Option<u32>,
    playback: Option<Playback>,
    position: u64,
    loop_range: Option<(u64, u64)>,
    paused: bool,
    playback_mask: Option<Vec<bool>>,
    playback_mapping: Option<Vec<Option<usize>>>,
    selected_info: Option<DeviceInfo>,
}
impl RecorderService {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        Ok(Self {
            root: root.as_ref().to_owned(),
            selected: None,
            take: None,
            capture: None,
            prepared: None,
            last_error: None,
            last_frames: 0,
            last_xruns: 0,
            rate: None,
            playback: None,
            position: 0,
            loop_range: None,
            paused: false,
            playback_mask: None,
            playback_mapping: None,
            selected_info: None,
        })
    }
    fn host() -> io::Result<cpal::Host> {
        #[cfg(target_os = "macos")]
        let id = cpal::HostId::CoreAudio;
        #[cfg(target_os = "windows")]
        let id = cpal::HostId::Asio;
        #[cfg(target_os = "linux")]
        let id = cpal::HostId::Alsa;
        cpal::host_from_id(id).map_err(error)
    }
    pub fn devices(&self) -> io::Result<Vec<DeviceInfo>> {
        #[cfg(target_os = "macos")]
        {
            crate::macos_devices::devices()
        }
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            let mut result = Vec::new();
            for d in Self::host()?.devices().map_err(error)? {
                let (Ok(id), Ok(desc)) = (d.id(), d.description()) else {
                    continue;
                };
                let input = d.default_input_config().ok();
                let output = d.default_output_config().ok();
                let Some(rate) = input
                    .as_ref()
                    .map(|c| c.sample_rate())
                    .or_else(|| output.as_ref().map(|c| c.sample_rate()))
                    .filter(|r| *r > 0)
                else {
                    continue;
                };
                let ins = d
                    .supported_input_configs()
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter(|c| {
                        c.channels() <= 64
                            && c.min_sample_rate() <= rate
                            && c.max_sample_rate() >= rate
                    })
                    .map(|c| c.channels())
                    .max()
                    .unwrap_or(0);
                let outs = if output.as_ref().is_some_and(|c| c.sample_rate() == rate) {
                    d.supported_output_configs()
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter(|c| {
                            c.channels() <= 64
                                && c.min_sample_rate() <= rate
                                && c.max_sample_rate() >= rate
                        })
                        .map(|c| c.channels())
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };
                if ins > 0 || outs > 0 {
                    result.push(DeviceInfo {
                        id: id.to_string(),
                        name: desc.name().to_owned(),
                        input_channels: ins,
                        output_channels: outs,
                        sample_rate: rate,
                    });
                }
            }
            Ok(result)
        }
    }
    pub fn select_device(&mut self, id: &str) -> io::Result<()> {
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("stop transport before selecting a device"));
        }
        let d = self
            .devices()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| error("device not found"))?;
        self.select_discovered_device(d)
    }
    /// Select a device from a recent enumeration performed by the caller.
    /// Opening streams still validates that the device is present.
    pub fn select_discovered_device(&mut self, d: DeviceInfo) -> io::Result<()> {
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("stop transport before selecting a device"));
        }
        if d.sample_rate == 0 {
            return Err(error("invalid device sample rate"));
        }
        self.rate = Some(d.sample_rate);
        self.selected = Some(d.id.clone());
        self.selected_info = Some(d);
        self.playback_mask = None;
        self.playback_mapping = None;
        Ok(())
    }
    pub fn status(&self) -> RecorderStatus {
        let c = self.capture.as_ref();
        RecorderStatus {
            selected_device_id: self.selected.clone(),
            selected_take_id: self.take.clone(),
            transport: if c.is_some() {
                "recording"
            } else if self.playback.as_ref().is_some_and(|p| {
                !p.ended.load(Ordering::Relaxed) && !p.done.load(Ordering::Relaxed)
            }) {
                "playing"
            } else if self.paused && self.playback.is_none() {
                "paused"
            } else {
                "stopped"
            }
            .into(),
            sample_rate: self.rate,
            recorded_frames: c.map_or(self.last_frames, |c| c.frames.load(Ordering::Relaxed)),
            position_frames: self
                .playback
                .as_ref()
                .map_or(self.position, |p| p.position.load(Ordering::Relaxed)),
            xruns: c.map_or(self.last_xruns, |c| c.xruns.load(Ordering::Relaxed))
                + self
                    .playback
                    .as_ref()
                    .map_or(0, |p| p.xruns.load(Ordering::Relaxed)),
            live_peaks: c.filter(|c| c.frames.load(Ordering::Relaxed) > 0).map(|c| {
                c.peaks[..c.channels]
                    .iter()
                    .map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
                    .collect()
            }),
            error: if self
                .playback
                .as_ref()
                .is_some_and(|p| p.done.load(Ordering::Relaxed))
            {
                Some("playback device or worker failed".into())
            } else if c.is_some_and(|c| c.failed.load(Ordering::Relaxed)) {
                Some("capture failed: device error, disk error, or queue overflow; stop and recover take".into())
            } else {
                self.last_error.clone()
            },
        }
    }
    pub fn takes(&self) -> io::Result<Vec<(String, TakeManifest)>> {
        let mut out = Vec::new();
        for e in fs::read_dir(&self.root)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                if let Ok(m) = read_manifest(e.path()) {
                    out.push((e.file_name().to_string_lossy().into_owned(), m));
                }
            }
        }
        Ok(out)
    }
    pub fn recover_take(&mut self, id: &str) -> io::Result<TakeManifest> {
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("transport active"));
        }
        valid_id(id)?;
        let dir = self.root.join(id);
        let mut manifest = recover(&dir)?;
        manifest.complete = true;
        crate::publish(&dir, &manifest)?;
        Ok(manifest)
    }
    /// Open a stopped input stream without creating any take files.
    /// This operation may block in the native driver and must run in an owned helper.
    pub fn prepare_capture(&mut self) -> io::Result<()> {
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("transport active"));
        }
        let selected = self
            .selected
            .as_ref()
            .ok_or_else(|| error("select a device first"))?;
        let device = Self::host()?
            .devices()
            .map_err(error)?
            .find(|d| d.id().is_ok_and(|i| i.to_string() == *selected))
            .ok_or_else(|| error("selected device disconnected"))?;
        let default = device.default_input_config().map_err(error)?;
        let rate = default.sample_rate();
        let channels = self
            .selected_info
            .as_ref()
            .ok_or_else(|| error("select a device first"))?
            .input_channels;
        ChannelLayout::new(channels)?;
        if rate == 0 {
            return Err(error("invalid device sample rate"));
        }
        let supported = device
            .supported_input_configs()
            .map_err(error)?
            .find(|c| {
                c.channels() == channels
                    && c.min_sample_rate() <= rate
                    && c.max_sample_rate() >= rate
            })
            .ok_or_else(|| {
                error("device does not support capture with this layout at its current sample rate")
            })?;
        let sample_format = supported.sample_format();
        let config = supported.with_sample_rate(rate).config();
        let (mut producer, consumer) =
            rtrb::RingBuffer::<[i32; MAX_CHANNELS]>::new(rate as usize * 4);
        let done = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let xruns = Arc::new(AtomicU64::new(0));
        let frames = Arc::new(AtomicU64::new(0));
        let peaks = Arc::new(std::array::from_fn(|_| AtomicU32::new(0)));
        let cb_failed = failed.clone();
        let cb_xruns = xruns.clone();
        let err_failed = failed.clone();
        macro_rules! build_input {
            ($sample:ty) => {
                device
                    .build_input_stream(
                        &config,
                        move |data: &[$sample], _| {
                            if cb_failed.load(Ordering::Relaxed) {
                                return;
                            }
                            if !data.len().is_multiple_of(channels as usize) {
                                cb_failed.store(true, Ordering::Release);
                                return;
                            }
                            for chunk in data.chunks_exact(channels as usize) {
                                let mut frame = [0; MAX_CHANNELS];
                                for (dst, &v) in frame.iter_mut().zip(chunk) {
                                    *dst = capture_sample(v);
                                }
                                if producer.push(frame).is_err() {
                                    cb_xruns.fetch_add(1, Ordering::Relaxed);
                                    cb_failed.store(true, Ordering::Release);
                                    break;
                                }
                            }
                        },
                        move |_| {
                            err_failed.store(true, Ordering::Release);
                        },
                        None,
                    )
                    .map_err(error)
            };
        }
        let stream = pcm_stream!(sample_format, build_input)?;
        self.prepared = Some(PreparedCapture {
            stream,
            consumer,
            done,
            failed,
            xruns,
            frames,
            peaks,
            rate,
            channels: channels as usize,
        });
        Ok(())
    }

    /// Commit an already prepared stream to a new take.
    pub fn commit_capture(&mut self, id: &str) -> io::Result<()> {
        valid_id(id)?;
        let PreparedCapture {
            stream,
            mut consumer,
            done,
            failed,
            xruns,
            frames,
            peaks,
            rate,
            channels,
        } = self
            .prepared
            .take()
            .ok_or_else(|| error("capture not prepared"))?;
        let mut writer = TakeWriter::create_with_layout(
            self.root.join(id),
            rate,
            48_000 * 60 * 10,
            ChannelLayout::new(channels as u16)?,
        )?;
        let worker_done = done.clone();
        let worker_failed = failed.clone();
        let worker_frames = frames.clone();
        let worker_peaks = peaks.clone();
        let worker = thread::spawn(move || {
            let mut batch = Vec::with_capacity(channels * 1024);
            let mut since_checkpoint = 0u64;
            loop {
                while batch.len() < channels * 1024 {
                    match consumer.pop() {
                        Ok(frame) => batch.extend_from_slice(&frame[..channels]),
                        Err(_) => break,
                    }
                }
                if !batch.is_empty() {
                    let n = (batch.len() / channels) as u64;
                    let mut maxima = [0u32; MAX_CHANNELS];
                    for frame in batch.chunks_exact(channels) {
                        for (max, &sample) in maxima.iter_mut().zip(frame) {
                            *max = (*max).max(sample.unsigned_abs());
                        }
                    }
                    for (slot, value) in worker_peaks.iter().zip(maxima) {
                        slot.store((value as f32 / 8_388_608.0).to_bits(), Ordering::Relaxed);
                    }
                    if let Err(e) = writer.append_interleaved(&batch) {
                        worker_failed.store(true, Ordering::Release);
                        return Err(e);
                    }
                    worker_frames.fetch_add(n, Ordering::Relaxed);
                    since_checkpoint += n;
                    batch.clear();
                    if since_checkpoint >= rate as u64 * 5 {
                        if let Err(e) = writer.checkpoint() {
                            worker_failed.store(true, Ordering::Release);
                            return Err(e);
                        }
                        since_checkpoint = 0;
                    }
                } else if worker_done.load(Ordering::Acquire)
                    || worker_failed.load(Ordering::Acquire)
                {
                    break;
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            if worker_failed.load(Ordering::Acquire) {
                writer.checkpoint()?;
                return Err(error("capture interrupted; recover take"));
            }
            writer.finish()
        });
        if let Err(e) = stream.play() {
            done.store(true, Ordering::Release);
            drop(stream);
            let _ = worker.join();
            return Err(error(e));
        }
        self.take = Some(id.to_owned());
        self.rate = Some(rate);
        self.last_error = None;
        self.capture = Some(Capture {
            stream,
            done,
            failed,
            xruns,
            frames,
            peaks,
            channels,
            worker,
        });
        Ok(())
    }

    pub fn start_recording(&mut self, id: &str) -> io::Result<()> {
        valid_id(id)?;
        self.prepare_capture()?;
        self.commit_capture(id)
    }

    fn stop_playback(&mut self) -> io::Result<()> {
        if let Some(p) = self.playback.take() {
            p.fade.store(true, Ordering::Release);
            for _ in 0..100 {
                if p.faded.load(Ordering::Acquire)
                    || p.ended.load(Ordering::Acquire)
                    || p.done.load(Ordering::Acquire)
                {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }
            let failed = p.done.load(Ordering::Acquire);
            drop(p.stream);
            p.done.store(true, Ordering::Release);
            self.position = p.position.load(Ordering::Acquire);
            self.last_xruns += p.xruns.load(Ordering::Relaxed);
            let result = p
                .worker
                .join()
                .map_err(|_| error("playback worker panicked"))?;
            if let Err(e) = result {
                self.last_error = Some(e.to_string());
                return Err(e);
            }
            if failed {
                let e = error("playback device failed");
                self.last_error = Some(e.to_string());
                return Err(e);
            }
        }
        Ok(())
    }
    pub fn set_playback_mask(&mut self, mask: Vec<bool>) -> io::Result<()> {
        if self.playback.is_some() {
            return Err(error("transport active"));
        }
        let count = self
            .selected_info
            .as_ref()
            .ok_or_else(|| error("select a device first"))?
            .output_channels as usize;
        ChannelLayout::new(count as u16)?;
        if mask.len() != count {
            return Err(error("output mask length differs from device channels"));
        }
        self.playback_mask = Some(mask);
        Ok(())
    }
    /// Map each take track to a distinct zero-based output channel.
    pub fn set_playback_mapping(&mut self, mapping: Vec<Option<usize>>) -> io::Result<()> {
        let outputs = self
            .selected_info
            .as_ref()
            .ok_or_else(|| error("select a device first"))?
            .output_channels as usize;
        if self.playback.is_some()
            || mapping.is_empty()
            || mapping.len() > 64
            || mapping.iter().flatten().any(|&n| n >= outputs)
        {
            return Err(error("invalid output mapping"));
        }
        let mut seen = std::collections::HashSet::new();
        if !mapping.iter().flatten().all(|n| seen.insert(*n)) || seen.is_empty() {
            return Err(error("output channels must be unique"));
        }
        self.playback_mapping = Some(mapping);
        Ok(())
    }
    pub fn load_take(&mut self, id: &str) -> io::Result<TakeManifest> {
        valid_id(id)?;
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("transport active"));
        }
        let m = read_manifest(self.root.join(id))?;
        if !m.complete {
            return Err(error("take requires recovery"));
        }
        self.take = Some(id.to_owned());
        self.position = 0;
        self.loop_range = None;
        self.paused = false;
        Ok(m)
    }
    pub fn pause(&mut self) -> io::Result<()> {
        if self.playback.is_none() {
            return Err(error("playback is not active"));
        }
        self.stop_playback()?;
        self.paused = true;
        Ok(())
    }
    pub fn seek(&mut self, frame: u64) -> io::Result<()> {
        let id = self.take.as_ref().ok_or_else(|| error("no take loaded"))?;
        let m = read_manifest(self.root.join(id))?;
        if frame > m.frames {
            return Err(error("seek beyond end of take"));
        }
        let playing = self
            .playback
            .as_ref()
            .is_some_and(|p| !p.ended.load(Ordering::Acquire) && !p.done.load(Ordering::Acquire));
        self.stop_playback()?;
        self.position = frame;
        if playing {
            self.play()?;
        }
        Ok(())
    }
    pub fn set_loop(&mut self, start: u64, end: u64, enabled: bool) -> io::Result<()> {
        let id = self.take.as_ref().ok_or_else(|| error("no take loaded"))?;
        let m = read_manifest(self.root.join(id))?;
        if enabled && (start >= end || end > m.frames) {
            return Err(error("invalid loop range"));
        }
        let playing = self
            .playback
            .as_ref()
            .is_some_and(|p| !p.ended.load(Ordering::Acquire) && !p.done.load(Ordering::Acquire));
        self.stop_playback()?;
        self.loop_range = if enabled { Some((start, end)) } else { None };
        if playing {
            self.play()?;
        }
        Ok(())
    }
    fn build_playback_stream(
        device_id: &str,
        sample_rate: u32,
        mut output: PlaybackOutput,
        done: Arc<AtomicBool>,
    ) -> io::Result<cpal::Stream> {
        let device = Self::host()?
            .devices()
            .map_err(error)?
            .find(|d| d.id().is_ok_and(|i| i.to_string() == device_id))
            .ok_or_else(|| error("selected device disconnected"))?;
        let channels = output.mask.len() as u16;
        ChannelLayout::new(channels)?;
        #[cfg(target_os = "macos")]
        let config = {
            let info = crate::macos_devices::devices()?
                .into_iter()
                .find(|info| info.id == device_id)
                .ok_or_else(|| error("selected device disconnected"))?;
            validate_playback_config(info.output_channels, info.sample_rate, sample_rate)?;
            if info.output_channels != channels {
                return Err(error("device output layout changed"));
            }
            cpal::StreamConfig {
                channels,
                sample_rate,
                buffer_size: cpal::BufferSize::Default,
            }
        };
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let current = device.default_output_config().map_err(error)?.sample_rate();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        if current != sample_rate {
            return Err(error("device sample rate differs from take"));
        }
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let supported = device
            .supported_output_configs()
            .map_err(error)?
            .find(|c| {
                c.channels() == channels
                    && c.min_sample_rate() <= current
                    && c.max_sample_rate() >= current
            })
            .ok_or_else(|| {
                error("device does not support playback with this layout at take rate")
            })?;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let sample_format = supported.sample_format();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let config = supported.with_sample_rate(current).config();
        #[cfg(target_os = "macos")]
        let sample_format = cpal::SampleFormat::F32;
        macro_rules! build_output {
            ($sample:ty) => {
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [$sample], _| output.render(data),
                        move |_| {
                            done.store(true, Ordering::Release);
                        },
                        None,
                    )
                    .map_err(error)
            };
        }
        pcm_stream!(sample_format, build_output)
    }

    /// Validate and construct an output stream without starting it.
    pub fn probe_output(device_id: &str, sample_rate: u32) -> io::Result<()> {
        Self::probe_output_with(device_id, sample_rate, || {})
    }

    /// Probe an output and invoke `built` after construction, before dropping the stream.
    pub fn probe_output_with(
        device_id: &str,
        sample_rate: u32,
        built: impl FnOnce(),
    ) -> io::Result<()> {
        let info = Self::new(std::env::temp_dir())?
            .devices()?
            .into_iter()
            .find(|d| d.id == device_id)
            .ok_or_else(|| error("device not found"))?;
        let (_producer, consumer) = rtrb::RingBuffer::<PlaybackFrame>::new(1);
        let done = Arc::new(AtomicBool::new(false));
        let output = PlaybackOutput {
            consumer,
            done: done.clone(),
            fade: Arc::new(AtomicBool::new(false)),
            faded: Arc::new(AtomicBool::new(false)),
            ended: Arc::new(AtomicBool::new(false)),
            position: Arc::new(AtomicU64::new(0)),
            xruns: Arc::new(AtomicU64::new(0)),
            mask: vec![false; info.output_channels as usize],
            take_channels: 0,
            mapping: None,
            ramp: 0.0,
            underrun: false,
        };
        let stream = Self::build_playback_stream(device_id, sample_rate, output, done)?;
        built();
        drop(stream);
        Ok(())
    }
    pub fn play(&mut self) -> io::Result<()> {
        if self
            .playback
            .as_ref()
            .is_some_and(|p| p.ended.load(Ordering::Acquire))
        {
            self.stop_playback()?;
            self.position = 0;
        }
        if self.capture.is_some() || self.prepared.is_some() || self.playback.is_some() {
            return Err(error("transport active"));
        }
        let id = self.take.as_ref().ok_or_else(|| error("no take loaded"))?;
        let dir = self.root.join(id);
        let manifest = read_manifest(&dir)?;
        if !manifest.complete {
            return Err(error("take requires recovery"));
        }
        let selected = self
            .selected
            .as_ref()
            .ok_or_else(|| error("select a device first"))?;
        let outputs = self
            .selected_info
            .as_ref()
            .ok_or_else(|| error("select a device first"))?
            .output_channels as usize;
        ChannelLayout::new(outputs as u16)?;
        let mask = self
            .playback_mask
            .clone()
            .unwrap_or_else(|| vec![true; outputs]);
        if self
            .playback_mapping
            .as_ref()
            .is_some_and(|m| m.len() != manifest.channels as usize)
        {
            return Err(error("mapping must include every take track"));
        }
        if mask.len() != outputs
            || (self.playback_mapping.is_none() && manifest.channels as usize > outputs)
        {
            return Err(error(
                "take channels exceed device outputs or mask length differs",
            ));
        }
        let (producer, consumer) =
            rtrb::RingBuffer::<PlaybackFrame>::new(manifest.sample_rate as usize * 2);
        let done = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(self.position));
        let xruns = Arc::new(AtomicU64::new(0));
        let fade = Arc::new(AtomicBool::new(false));
        let faded = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let output = PlaybackOutput {
            consumer,
            done: done.clone(),
            fade: fade.clone(),
            faded: faded.clone(),
            ended: ended.clone(),
            position: position.clone(),
            xruns: xruns.clone(),
            mask,
            take_channels: manifest.channels as usize,
            mapping: self.playback_mapping.as_ref().map(|mapping| {
                let mut inverted = vec![None; outputs];
                for (track, output) in mapping.iter().enumerate() {
                    if let Some(output) = output {
                        inverted[*output] = Some(track);
                    }
                }
                inverted
            }),
            ramp: 0.0,
            underrun: false,
        };
        let stream =
            Self::build_playback_stream(selected, manifest.sample_rate, output, done.clone())?;
        let worker_done = done.clone();
        let ready = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let at = self.position;
        let looping = self.loop_range;
        let worker = thread::spawn(move || {
            playback_worker(
                producer,
                dir,
                manifest,
                at,
                looping,
                worker_done,
                worker_ready,
            )
        });
        for _ in 0..1000 {
            if ready.load(Ordering::Acquire) || done.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        if !ready.load(Ordering::Acquire) {
            done.store(true, Ordering::Release);
            drop(stream);
            return match worker.join() {
                Ok(Err(e)) => Err(e),
                _ => Err(error("playback prefetch timed out")),
            };
        }
        if let Err(e) = stream.play() {
            done.store(true, Ordering::Release);
            drop(stream);
            let _ = worker.join();
            return Err(error(e));
        }
        self.paused = false;
        self.last_error = None;
        self.playback = Some(Playback {
            stream,
            done,
            fade,
            faded,
            ended,
            position,
            xruns,
            worker,
        });
        Ok(())
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.prepared.take();
        self.stop_playback()?;
        self.paused = false;
        self.position = 0;
        if let Some(c) = self.capture.take() {
            drop(c.stream);
            c.done.store(true, Ordering::Release);
            let result = c
                .worker
                .join()
                .map_err(|_| error("capture worker panicked"))?;
            self.last_frames = c.frames.load(Ordering::Relaxed);
            self.last_xruns = c.xruns.load(Ordering::Relaxed);
            if let Err(e) = result {
                self.last_error = Some(e.to_string());
                return Err(e);
            }
        }
        Ok(())
    }
}
impl Drop for RecorderService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn playback_configuration_requires_channels_and_matching_rate() {
        assert!(super::validate_playback_config(0, 48_000, 48_000).is_err());
        assert!(super::validate_playback_config(18, 44_100, 48_000).is_err());
        assert!(super::validate_playback_config(18, 48_000, 48_000).is_ok());
    }

    use super::*;
    use crate::CHANNELS;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "openfader-service-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn output(consumer: rtrb::Consumer<PlaybackFrame>) -> PlaybackOutput {
        PlaybackOutput {
            consumer,
            done: Arc::new(AtomicBool::new(false)),
            fade: Arc::new(AtomicBool::new(false)),
            faded: Arc::new(AtomicBool::new(false)),
            ended: Arc::new(AtomicBool::new(false)),
            position: Arc::new(AtomicU64::new(0)),
            xruns: Arc::new(AtomicU64::new(0)),
            mask: vec![true; CHANNELS],
            take_channels: CHANNELS,
            mapping: None,
            ramp: 1.0,
            underrun: false,
        }
    }
    #[test]
    fn mapped_output_routes_tracks_and_leaves_unassigned_silent() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(2);
        let mut samples = [0.0; MAX_CHANNELS];
        samples[0] = 0.25;
        samples[1] = 0.5;
        samples[2] = -0.75;
        producer.push(Some((0, samples))).unwrap();
        let mut cb = output(consumer);
        cb.mask = vec![true; 3];
        cb.mapping = Some(vec![Some(2), None, Some(0)]);
        cb.take_channels = 3;
        let mut data = [0.0f32; 3];
        cb.render(&mut data);
        assert_eq!(data, [-0.75, 0.0, 0.25]);
    }
    #[test]
    fn native_integer_playback_converts_and_silences_masked_channels() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(4);
        let mut samples = [0.0; MAX_CHANNELS];
        samples[0] = 0.5;
        samples[1] = -0.5;
        samples[2] = 0.75;
        producer.push(Some((0, samples))).unwrap();
        let mut output = output(consumer);
        output.mask[2] = false;
        let mut data = [123i32; CHANNELS];
        output.render(&mut data);
        assert_eq!(data[0], 1 << 30);
        assert_eq!(data[1], -(1 << 30));
        assert_eq!(data[2], 0);
        assert_eq!(output.position.load(Ordering::Acquire), 1);
        let mut silence = [0u16; CHANNELS];
        output.render(&mut silence);
        assert!(silence.iter().all(|&v| v == 32768));
    }

    #[test]
    fn variable_output_strides_and_masks_preserve_alignment() {
        for channels in [16, 18, 32] {
            let (mut producer, consumer) = rtrb::RingBuffer::new(2);
            let mut cb = output(consumer);
            cb.mask = vec![true; channels];
            cb.take_channels = channels;
            cb.mask[channels - 1] = false;
            let frame = std::array::from_fn(|i| i as f32 + 1.0);
            producer.push(Some((0, frame))).unwrap();
            producer.push(Some((1, frame))).unwrap();
            let mut samples = vec![9.0; channels * 2 + 1];
            cb.render(&mut samples);
            for chunk in samples[..channels * 2].chunks_exact(channels) {
                assert_eq!(&chunk[..channels - 1], &frame[..channels - 1]);
                assert_eq!(chunk[channels - 1], 0.0);
            }
            assert_eq!(samples[channels * 2], 0.0);
            assert_eq!(cb.position.load(Ordering::Relaxed), 2);
        }
    }
    #[test]
    fn mask_must_match_selected_output_stride() {
        let root = root();
        let mut service = RecorderService::new(&root).unwrap();
        service
            .select_discovered_device(DeviceInfo {
                id: "test".into(),
                name: "test".into(),
                input_channels: 16,
                output_channels: 32,
                sample_rate: 48_000,
            })
            .unwrap();
        assert!(service.set_playback_mask(vec![true; 18]).is_err());
        assert!(service.set_playback_mask(vec![true; 32]).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn masked_output_preserves_frame_stride_and_position() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(3);
        let mut cb = output(consumer);
        cb.mask[14..].fill(false);
        let first = std::array::from_fn(|i| i as f32 + 1.0);
        let second = std::array::from_fn(|i| i as f32 + 21.0);
        producer.push(Some((4, first))).unwrap();
        producer.push(Some((5, second))).unwrap();
        let mut samples = [0.0; CHANNELS * 2];
        cb.render(&mut samples);
        assert_eq!(&samples[..14], &first[..14]);
        assert_eq!(&samples[CHANNELS..CHANNELS + 14], &second[..14]);
        assert!(samples[14..CHANNELS]
            .iter()
            .chain(&samples[CHANNELS + 14..])
            .all(|x| *x == 0.0));
        assert_eq!(cb.position.load(Ordering::Acquire), 6);
    }
    #[test]
    fn eof_underrun_and_failure_silence_output_without_losing_position() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(2);
        let mut cb = output(consumer);
        producer.push(Some((4, [0.5; MAX_CHANNELS]))).unwrap();
        let mut samples = [9.0; CHANNELS * 3];
        cb.render(&mut samples);
        assert_eq!(samples[0], 0.5);
        assert!(samples[CHANNELS..].iter().all(|v| *v == 0.0));
        assert_eq!(cb.position.load(Ordering::Relaxed), 5);
        assert_eq!(cb.xruns.load(Ordering::Relaxed), 1);
        cb.render(&mut samples);
        assert_eq!(cb.xruns.load(Ordering::Relaxed), 1);
        producer.push(Some((5, [0.5; MAX_CHANNELS]))).unwrap();
        producer.push(None).unwrap();
        cb.render(&mut samples);
        assert!(cb.ended.load(Ordering::Relaxed));
        assert_eq!(cb.position.load(Ordering::Relaxed), 6);
        assert!(samples[CHANNELS..].iter().all(|v| *v == 0.0));
        assert_eq!(cb.xruns.load(Ordering::Relaxed), 1);
        let (mut producer, consumer) = rtrb::RingBuffer::new(1);
        let mut cb = output(consumer);
        producer.push(Some((8, [1.0; MAX_CHANNELS]))).unwrap();
        cb.done.store(true, Ordering::Release);
        cb.render(&mut samples);
        assert!(samples.iter().all(|v| *v == 0.0));
        assert_eq!(cb.position.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn generated_take_worker_delivers_eof_and_reports_read_failure() {
        let root = root();
        let mut writer = TakeWriter::create(
            &root,
            48_000,
            8,
            ChannelLayout::new(CHANNELS as u16).unwrap(),
        )
        .unwrap();
        writer.append_interleaved(&[100; CHANNELS * 2]).unwrap();
        let manifest = writer.finish().unwrap();
        let (producer, mut consumer) = rtrb::RingBuffer::new(4);
        let done = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        playback_worker(
            producer,
            root.clone(),
            manifest.clone(),
            1,
            None,
            done.clone(),
            ready.clone(),
        )
        .unwrap();
        assert_eq!(consumer.pop().unwrap().unwrap().0, 1);
        assert!(consumer.pop().unwrap().is_none());
        assert!(ready.load(Ordering::Acquire));
        fs::remove_file(root.join(&manifest.segments[0].file)).unwrap();
        let (producer, _consumer) = rtrb::RingBuffer::new(4);
        assert!(playback_worker(
            producer,
            root.clone(),
            manifest,
            0,
            None,
            done.clone(),
            ready
        )
        .is_err());
        assert!(done.load(Ordering::Acquire));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn generated_take_worker_seeks_loops_and_stops_with_full_queue() {
        let root = root();
        let mut writer = TakeWriter::create(
            &root,
            48_000,
            2,
            ChannelLayout::new(CHANNELS as u16).unwrap(),
        )
        .unwrap();
        for frame in 1..=4 {
            writer
                .append_interleaved(&[frame * 100_000; CHANNELS])
                .unwrap();
        }
        let manifest = writer.finish().unwrap();
        let (producer, mut consumer) = rtrb::RingBuffer::new(2);
        let done = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let worker_done = done.clone();
        let dir = root.clone();
        let worker = thread::spawn(move || {
            playback_worker(producer, dir, manifest, 2, Some((1, 4)), worker_done, ready)
        });
        let mut frames = Vec::new();
        for _ in 0..2000 {
            if let Ok(Some((frame, samples))) = consumer.pop() {
                frames.push(frame);
                assert!(
                    (samples[0] - (frame + 1) as f32 * 100_000.0 / 8_388_608.0).abs() < 0.000001
                );
                if frames.len() == 6 {
                    break;
                }
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }
        assert_eq!(frames, [2, 3, 1, 2, 3, 1]);
        // The producer can be blocked by a full bounded queue and must still stop.
        thread::sleep(Duration::from_millis(5));
        done.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn take_transport_validates_boundaries_without_opening_a_device() {
        let root = root();
        let mut service = RecorderService::new(&root).unwrap();
        let mut writer = TakeWriter::create(
            root.join("take"),
            48_000,
            8,
            ChannelLayout::new(CHANNELS as u16).unwrap(),
        )
        .unwrap();
        writer.append_interleaved(&[7; CHANNELS * 4]).unwrap();
        writer.finish().unwrap();
        assert_eq!(service.load_take("take").unwrap().frames, 4);
        assert!(service.seek(5).is_err());
        assert!(service.set_loop(2, 2, true).is_err());
        service.set_loop(1, 4, true).unwrap();
        service.seek(3).unwrap();
        assert_eq!(service.status().position_frames, 3);
        assert!(service.play().is_err());
        service.stop().unwrap();
        assert_eq!(service.status().position_frames, 0);
        assert!(service.load_take("../take").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod native_format_tests {
    use super::*;
    #[test]
    fn integer_capture_preserves_24_bit_alignment_and_limits() {
        assert_eq!(capture_sample(i32::MIN), -8_388_608);
        assert_eq!(capture_sample(i32::MAX), 8_388_607);
        assert_eq!(capture_sample(0i32), 0);
        assert_eq!(capture_sample(123456i32 << 8), 123456);
        assert_eq!(capture_sample(-123456i32 << 8), -123456);
        assert_eq!(capture_sample(i16::MIN), -8_388_608);
        assert_eq!(capture_sample(0u16), -8_388_608);
        assert_eq!(capture_sample(32768u16), 0);
        assert_eq!(capture_sample(f32::NAN), 0);
        assert_eq!(capture_sample(0.5f64), 4_194_304);
    }
}
