//! Isolated XR18 diagnostic. Confirm host recorder is stopped and journal mixer
//! selectors before use. Usage: xr18_duplex_probe <USB output 1..18> [--level-db -60].
//! Output is zero for one second, then a 997 Hz sine at -96 dBFS peak for
//! two seconds, then zero. The optional level is limited to -120..=-60 dBFS.
//! This program never changes mixer settings.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };
    const N: usize = 18;
    const RATE: u32 = 48_000;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let level_db: f64 = if let Some(i) = args.iter().position(|a| a == "--level-db") {
        args.get(i + 1).ok_or("missing level")?.parse()?
    } else {
        -96.0
    };
    if !level_db.is_finite() || !(-120.0..=-60.0).contains(&level_db) {
        return Err("level must be between -120 and -60 dBFS".into());
    }
    if args.first().map(String::as_str) == Some("--self-test") {
        let signal: Vec<f64> = (0..RATE)
            .map(|i| {
                10f64.powf(level_db / 20.0)
                    * (std::f64::consts::TAU * 997.0 * i as f64 / RATE as f64).sin()
            })
            .collect();
        let (sin, cos) = signal
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |(s, c), (i, x)| {
                let p = std::f64::consts::TAU * 997.0 * i as f64 / RATE as f64;
                (s + x * p.sin(), c + x * p.cos())
            });
        let measured = 2.0 * sin.hypot(cos) / signal.len() as f64;
        let expected = 10f64.powf(level_db / 20.0);
        assert!((measured - expected).abs() < 1e-12);
        println!("synthetic 997Hz marker peak={measured:.9} expected={expected:.9}");
        return Ok(());
    }
    let channel: usize = std::env::args()
        .nth(1)
        .ok_or("supply output channel 1..18")?
        .parse()?;
    if !(1..=N).contains(&channel) {
        return Err("output channel must be 1..18".into());
    }
    let host = cpal::default_host();
    let devices: Vec<_> = host
        .devices()?
        .filter(|d| {
            d.description()
                .is_ok_and(|v| v.name().to_ascii_lowercase().contains("xr18"))
        })
        .collect();
    if devices.len() != 1 {
        return Err("expected exactly one XR18 device".into());
    }
    let device = &devices[0];
    let config = |input: bool| -> Result<cpal::StreamConfig, Box<dyn std::error::Error>> {
        let mut ranges: Box<dyn Iterator<Item = cpal::SupportedStreamConfigRange>> = if input {
            Box::new(device.supported_input_configs()?)
        } else {
            Box::new(device.supported_output_configs()?)
        };
        Ok(ranges
            .find(|c| {
                c.channels() == N as u16
                    && c.sample_format() == cpal::SampleFormat::F32
                    && c.min_sample_rate() <= RATE
                    && c.max_sample_rate() >= RATE
            })
            .ok_or("18-channel float 48 kHz unsupported")?
            .with_sample_rate(RATE)
            .config())
    };
    let samples = Arc::new(Mutex::new(Vec::<[f32; N]>::with_capacity(
        RATE as usize * 5,
    )));
    let failed = Arc::new(AtomicBool::new(false));
    let captured = samples.clone();
    let err = failed.clone();
    let input = device.build_input_stream(
        config(true)?,
        move |data: &[f32], _| {
            if let Ok(mut v) = captured.try_lock() {
                for frame in data.as_chunks::<N>().0 {
                    if v.len() < RATE as usize * 5 {
                        v.push(*frame);
                    }
                }
            }
        },
        move |e| {
            eprintln!("input error: {e}");
            err.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    let mut frame = 0usize;
    let err = failed.clone();
    let output = device.build_output_stream(
        config(false)?,
        move |data: &mut [f32], _| {
            data.fill(0.0);
            for chunk in data.as_chunks_mut::<N>().0 {
                if (RATE as usize..RATE as usize * 3).contains(&frame) {
                    chunk[channel - 1] = 10f32.powf(level_db as f32 / 20.0)
                        * (std::f32::consts::TAU * 997.0 * frame as f32 / RATE as f32).sin();
                }
                frame += 1;
            }
        },
        move |e| {
            eprintln!("output error: {e}");
            err.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    input.play()?;
    output.play()?;
    std::thread::sleep(Duration::from_secs(4));
    drop(output);
    drop(input); // Stop audio before reporting or any external restore.
    if failed.load(Ordering::Relaxed) {
        return Err("audio stream error; results invalid".into());
    }
    let samples = samples.lock().map_err(|_| "capture lock poisoned")?;
    if samples.len() < RATE as usize * 3 {
        return Err("insufficient capture; results invalid".into());
    }
    println!("frames={}, output={}, marker=997Hz peak {level_db}dBFS; windows refer to capture clock, streams may start asynchronously", samples.len(), channel);
    for c in 0..N {
        for (label, start, end) in [
            ("baseline", 0, RATE as usize / 2),
            ("marker", RATE as usize * 3 / 2, RATE as usize * 5 / 2),
        ] {
            let mut sum = 0.0f64;
            let mut sin = 0.0f64;
            let mut cos = 0.0f64;
            for (i, f) in samples[start..end].iter().enumerate() {
                let x = f[c] as f64;
                let phase = std::f64::consts::TAU * 997.0 * i as f64 / RATE as f64;
                sum += x * x;
                sin += x * phase.sin();
                cos += x * phase.cos();
            }
            let n = (end - start) as f64;
            println!(
                "ch={} {} rms={:.9} tone_peak={:.9}",
                c + 1,
                label,
                (sum / n).sqrt(),
                2.0 * sin.hypot(cos) / n
            );
        }
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("XR18 duplex probe requires macOS CoreAudio");
}
