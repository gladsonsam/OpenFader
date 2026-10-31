#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use openfader_audio::{RecorderService, CHANNELS};
    use std::{
        fs, thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "openfader-capture-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    ));
    let mut service = RecorderService::new(&root)?;
    let devices = service.devices()?;
    for d in &devices {
        println!(
            "{}: {} ({} in, {} out, {} Hz)",
            d.id, d.name, d.input_channels, d.output_channels, d.sample_rate
        );
    }
    let device = devices
        .iter()
        .find(|d| d.name.to_ascii_lowercase().contains("xr18"))
        .ok_or("XR18 18-in/18-out device not found")?;
    service.select_device(&device.id)?;
    service.start_recording("smoke").map_err(|e| {
        format!("capture could not start (check microphone permission and device access): {e}")
    })?;
    thread::sleep(Duration::from_secs(5));
    service.stop().map_err(|e| {
        format!("capture failed (check microphone permission and device connection): {e}")
    })?;
    let m = openfader_audio::read_manifest(root.join("smoke"))?;
    if m.frames == 0 || !m.complete {
        return Err("capture produced no complete frames; check microphone permission".into());
    }
    for segment in &m.segments {
        let data = fs::read(root.join("smoke").join(&segment.file))?;
        if &data[..4] != b"RIFF"
            || &data[8..12] != b"WAVE"
            || u16::from_le_bytes([data[22], data[23]]) != CHANNELS as u16
            || u16::from_le_bytes([data[34], data[35]]) != 24
            || data.len() != 44 + segment.frames as usize * CHANNELS * 3
        {
            return Err("invalid WAV header or frame alignment".into());
        }
    }
    println!(
        "Captured {} aligned frames at {} Hz in {}",
        m.frames,
        m.sample_rate,
        root.display()
    );
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("CoreAudio or ASIO required");
}
