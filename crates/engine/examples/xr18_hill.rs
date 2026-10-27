//! Hill-climb harness: time a real-desk connect + hydration over the engine.
//!
//! ```sh
//! cargo run --quiet --example xr18_hill -- 192.168.40.123
//! ```
//!
//! Reports milestone timings (link/caps/state/first-params/names/fx/routing)
//! and quiescence, so hydration optimizations are measurable run over run.
//! Read-only: sends `probe` only, never writes mixer state.
use openfader_engine::{Cmd, Engine, EngineConfig};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "192.168.40.123".to_string());
    let engine = Engine::new(EngineConfig::default());
    let mut client = engine.attach(Some("hill".to_string())).await;

    // Drain the attach snapshot first so probe frames are the timed ones.
    while client.events.try_recv().is_ok() {}

    let t0 = Instant::now();
    client.send(Cmd::Probe { addr: addr.clone() }).await?;

    let mut t_link: Option<Duration> = None;
    let mut t_caps: Option<Duration> = None;
    let mut t_state: Option<Duration> = None;
    let mut t_first_params: Option<Duration> = None;
    let mut t_ch_names: Option<Duration> = None;
    let mut t_fx: Option<Duration> = None;
    let mut t_routing: Option<Duration> = None;
    let mut t_masters: Option<Duration> = None;
    let mut params_frames = 0usize;
    let mut control_frames = 0usize;
    let mut stream_frames = 0usize;
    let mut names: HashMap<u16, String> = HashMap::new();
    let mut faders: HashSet<u16> = HashSet::new();
    let mut errors: Vec<String> = Vec::new();

    let quiet_for = Duration::from_secs(2);
    let hard_cap = Duration::from_secs(30);
    let mut last_params = Instant::now();

    loop {
        let elapsed = t0.elapsed();
        if elapsed > hard_cap {
            println!("HILL: hard cap {hard_cap:?} hit");
            break;
        }
        // Done when the fill's headline frames have all landed and the
        // params stream has gone quiet. `link` rebroadcasts periodically
        // on its own and never counts toward quiet.
        let params_quiet = last_params.elapsed() >= quiet_for;
        if t_routing.is_some() && params_quiet && elapsed > Duration::from_secs(5) {
            println!("HILL: fill complete (routing + params quiet for {quiet_for:?})");
            break;
        }
        let frame =
            match tokio::time::timeout(Duration::from_millis(200), client.events.recv()).await {
                Ok(Some(f)) => f,
                Ok(None) => {
                    println!("HILL: mailbox closed");
                    break;
                }
                Err(_) => continue,
            };
        let t = t0.elapsed();
        match frame.tag() {
            "meters" | "taps" | "spectrum" | "audio_spectrum" | "gr" | "dynamics_meters"
            | "output_meters" => {
                stream_frames += 1;
                continue;
            }
            "link" => {
                // Periodic rebroadcast; not load progress.
                control_frames += 1;
                if t_link.is_none() && frame.json.contains("\"connected\"") {
                    t_link = Some(t);
                    println!("HILL t={t:6.2?} link connected=true");
                }
            }
            "caps" => {
                control_frames += 1;
                if t_caps.is_none() {
                    t_caps = Some(t);
                }
                println!("HILL t={t:6.2?} caps");
            }
            "state" => {
                control_frames += 1;
                if t_state.is_none() {
                    t_state = Some(t);
                }
                let v: serde_json::Value = serde_json::from_str(&frame.json).unwrap_or_default();
                let n = v
                    .get("strips")
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!("HILL t={t:6.2?} state strips={n}");
            }
            "params" => {
                last_params = Instant::now();
                control_frames += 1;
                params_frames += 1;
                if t_first_params.is_none() {
                    t_first_params = Some(t);
                    println!("HILL t={t:6.2?} first params");
                }
                let v: serde_json::Value = serde_json::from_str(&frame.json).unwrap_or_default();
                let ch = v.get("ch").and_then(|c| c.as_u64()).unwrap_or(999) as u16;
                if let Some(vals) = v.get("values") {
                    if let Some(name) = vals.get("name").and_then(|n| n.as_str()) {
                        names.insert(ch, name.to_string());
                        if t_ch_names.is_none() && (0..16u16).all(|c| names.contains_key(&c)) {
                            t_ch_names = Some(t);
                            println!("HILL t={t:6.2?} all 16 ch names");
                        }
                    }
                    if vals.get("fader").is_some() {
                        faders.insert(ch);
                    }
                }
            }
            "param" | "bus" | "subgroup" | "main" | "solo" => {
                last_params = Instant::now();
                control_frames += 1;
                if t_masters.is_none() && matches!(frame.tag(), "bus" | "subgroup" | "main") {
                    t_masters = Some(t);
                }
            }
            "fx" => {
                control_frames += 1;
                if t_fx.is_none() {
                    t_fx = Some(t);
                    println!("HILL t={t:6.2?} fx");
                }
            }
            "routing" => {
                control_frames += 1;
                if t_routing.is_none() {
                    t_routing = Some(t);
                    println!("HILL t={t:6.2?} routing");
                }
            }
            "error" => {
                control_frames += 1;
                errors.push(frame.json.to_string());
                println!("HILL t={t:6.2?} ERROR {}", frame.json);
            }
            other => {
                control_frames += 1;
                println!("HILL t={t:6.2?} {other}");
            }
        }
    }

    let named_real = names
        .values()
        .filter(|n| {
            let n = n.trim();
            !n.is_empty() && !regex_lite(n)
        })
        .count();
    println!("HILL summary addr={addr}");
    println!("HILL   t_link={} t_caps={} t_state={} t_first_params={} t_ch_names={} t_masters={} t_fx={} t_routing={} t_quiescent={:.2?}",
        fmt(t_link), fmt(t_caps), fmt(t_state), fmt(t_first_params), fmt(t_ch_names), fmt(t_masters), fmt(t_fx), fmt(t_routing), t0.elapsed());
    println!("HILL   control_frames={control_frames} params_frames={params_frames} stream_frames={stream_frames}");
    println!(
        "HILL   strips_with_fader={} strips_named={}/{} errors={}",
        faders.len(),
        named_real,
        names.len(),
        errors.len()
    );
    Ok(())
}

fn fmt(d: Option<Duration>) -> String {
    d.map(|d| format!("{:.2?}", d))
        .unwrap_or_else(|| "—".to_string())
}

/// Placeholder topology names look like CH1/CH09/FX Return 1; treat anything
/// else as a real desk-provided name.
fn regex_lite(n: &str) -> bool {
    let n = n.trim();
    (n.starts_with("CH") && n[2..].trim().parse::<u32>().is_ok())
        || n.starts_with("FX Return")
        || n.starts_with("Return")
        || n.starts_with("VCA")
        || n.starts_with("Bus")
        || n == "Main"
}
