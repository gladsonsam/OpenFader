//! openfader-cli: headless demo of any `MixerDriver`.
//!
//! Uses sim by default; pass `--driver xair|dl32s --addr <ip>` for hardware.
//! Channels are 1-based in the CLI (`1..=N`) and map to 0-based `StripId`.
use openfader_core::{MixerDriver, ParamValue, StripId};
use std::time::Duration;

const HELP: &str = "\
Usage: openfader-cli [--driver sim|xair|dl32s] [--addr IP] [--preset generic|xr18|dl32s|studio] <command> [args]

Commands:
  list                       list all strips (name, fader, mute)
  fader <ch> <0-100>         set channel fader (1-based channel number)
  mute <ch> <on|off>         set channel mute
  name <ch> <name>           set channel name (sim only)
  param <ch> <id> [value]    get/set any catalogue param in engineering units
                             (e.g. `param 2 pan 25`); with no value, reads back
  meters                     print one fake meter frame
  discover [--timeout MS]    X-Air LAN discovery (/xinfo broadcast)

Options:
  --driver <name>   sim (default), xair, dl32s
  --addr <ip>       mixer address (xair/dl32s connect; sim ignores)
  --preset <name>   sim scribbles: generic (default), xr18, dl32s, studio (owner rig)
  -h, --help        print this help
";

fn parse_ch(s: &str, count: usize) -> anyhow::Result<StripId> {
    let ch: usize = s
        .parse()
        .map_err(|_| anyhow::anyhow!("channel must be a number 1..={count}, got {s:?}"))?;
    if ch == 0 || ch > count {
        anyhow::bail!("channel must be 1..={count}, got {ch}");
    }
    Ok(StripId((ch - 1) as u16))
}

fn parse_bool_word(raw: &str) -> anyhow::Result<bool> {
    match raw.to_lowercase().as_str() {
        "on" | "1" | "true" => Ok(true),
        "off" | "0" | "false" => Ok(false),
        _ => anyhow::bail!("boolean needs on|off, got {raw:?}"),
    }
}

/// Build a `ParamValue` for `id` from one CLI word, typed by the canonical
/// catalogue (`float`/`enum` → number, `bool` → on/off words, `text` → raw
/// string). Unknown ids (desk leaves this catalogue does not name yet)
/// fall back to number → bool-word → string, so the CLI can already drive
/// them on hardware that answers without a per-id subcommand.
fn parse_param_value(id: &str, raw: &str) -> anyhow::Result<ParamValue> {
    use openfader_core::params::{find, ParamKind};
    match find(id).map(|d| d.kind) {
        Some(ParamKind::Bool) => parse_bool_word(raw).map(ParamValue::Bool),
        Some(ParamKind::Text) => Ok(ParamValue::Text(raw.to_string())),
        Some(ParamKind::Float | ParamKind::Enum) => raw
            .parse::<f32>()
            .map(ParamValue::Float)
            .map_err(|_| anyhow::anyhow!("{id} needs a number, got {raw:?}")),
        None => {
            if let Ok(v) = raw.parse::<f32>() {
                return Ok(ParamValue::Float(v));
            }
            if let Ok(b) = parse_bool_word(raw) {
                return Ok(ParamValue::Bool(b));
            }
            Ok(ParamValue::Text(raw.to_string()))
        }
    }
}

/// Static dispatch over drivers (`MixerDriver` is not dyn-compatible due to
/// async methods). New providers add one variant + one delegation arm.
enum AnyDriver {
    Sim(Box<openfader_sim::SimDriver>),
    XAir(Box<openfader_proto_xair::XAirDriver>),
    Dl32s(Box<openfader_proto_dl32s::Dl32sDriver>),
}

impl MixerDriver for AnyDriver {
    fn name(&self) -> &'static str {
        match self {
            AnyDriver::Sim(d) => d.name(),
            AnyDriver::XAir(d) => d.name(),
            AnyDriver::Dl32s(d) => d.name(),
        }
    }
    fn channel_count(&self) -> usize {
        match self {
            AnyDriver::Sim(d) => d.channel_count(),
            AnyDriver::XAir(d) => d.channel_count(),
            AnyDriver::Dl32s(d) => d.channel_count(),
        }
    }
    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.connect(addr).await,
            AnyDriver::XAir(d) => d.connect(addr).await,
            AnyDriver::Dl32s(d) => d.connect(addr).await,
        }
    }
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_fader(strip, value).await,
            AnyDriver::XAir(d) => d.set_fader(strip, value).await,
            AnyDriver::Dl32s(d) => d.set_fader(strip, value).await,
        }
    }
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        match self {
            AnyDriver::Sim(d) => d.get_fader(strip).await,
            AnyDriver::XAir(d) => d.get_fader(strip).await,
            AnyDriver::Dl32s(d) => d.get_fader(strip).await,
        }
    }
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_mute(strip, muted).await,
            AnyDriver::XAir(d) => d.set_mute(strip, muted).await,
            AnyDriver::Dl32s(d) => d.set_mute(strip, muted).await,
        }
    }
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        match self {
            AnyDriver::Sim(d) => d.get_mute(strip).await,
            AnyDriver::XAir(d) => d.get_mute(strip).await,
            AnyDriver::Dl32s(d) => d.get_mute(strip).await,
        }
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_name(strip, name).await,
            AnyDriver::XAir(d) => d.set_name(strip, name).await,
            AnyDriver::Dl32s(d) => d.set_name(strip, name).await,
        }
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        match self {
            AnyDriver::Sim(d) => d.get_name(strip).await,
            AnyDriver::XAir(d) => d.get_name(strip).await,
            AnyDriver::Dl32s(d) => d.get_name(strip).await,
        }
    }
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_param(strip, param, value).await,
            AnyDriver::XAir(d) => d.set_param(strip, param, value).await,
            AnyDriver::Dl32s(d) => d.set_param(strip, param, value).await,
        }
    }
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match self {
            AnyDriver::Sim(d) => d.get_param(strip, param).await,
            AnyDriver::XAir(d) => d.get_param(strip, param).await,
            AnyDriver::Dl32s(d) => d.get_param(strip, param).await,
        }
    }
    async fn snapshot(&self) -> anyhow::Result<openfader_core::MixerSnapshot> {
        match self {
            AnyDriver::Sim(d) => d.snapshot().await,
            AnyDriver::XAir(d) => d.snapshot().await,
            AnyDriver::Dl32s(d) => d.snapshot().await,
        }
    }
    async fn recall(&self, snapshot: &openfader_core::MixerSnapshot) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.recall(snapshot).await,
            AnyDriver::XAir(d) => d.recall(snapshot).await,
            AnyDriver::Dl32s(d) => d.recall(snapshot).await,
        }
    }
    async fn query_info(&self) -> anyhow::Result<openfader_core::MixerInfo> {
        match self {
            AnyDriver::Sim(d) => d.query_info().await,
            AnyDriver::XAir(d) => d.query_info().await,
            AnyDriver::Dl32s(d) => d.query_info().await,
        }
    }
    fn poll_meters(&self) -> openfader_core::MeterFrame {
        match self {
            AnyDriver::Sim(d) => d.poll_meters(),
            AnyDriver::XAir(d) => d.poll_meters(),
            AnyDriver::Dl32s(d) => d.poll_meters(),
        }
    }
}

fn make_driver(kind: &str, preset: openfader_sim::SimPreset) -> AnyDriver {
    match kind {
        "xair" => AnyDriver::XAir(Box::new(openfader_proto_xair::XAirDriver::new())),
        "dl32s" => AnyDriver::Dl32s(Box::new(openfader_proto_dl32s::Dl32sDriver::new())),
        _ => AnyDriver::Sim(Box::new(openfader_sim::SimDriver::with_preset(preset))),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // Global flags (may appear before the command).
    let mut driver = "sim".to_string();
    let mut addr: Option<String> = None;
    let mut preset = openfader_sim::SimPreset::Generic;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--driver" => {
                i += 1;
                driver = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--driver needs a value"))?;
            }
            v if v.starts_with("--driver=") => {
                driver = v["--driver=".len()..].to_string();
            }
            "--addr" => {
                i += 1;
                addr = Some(
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--addr needs a value"))?,
                );
            }
            v if v.starts_with("--addr=") => {
                addr = Some(v["--addr=".len()..].to_string());
            }
            "--preset" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--preset needs a value"))?;
                preset = openfader_sim::SimPreset::parse(&raw).ok_or_else(|| {
                    anyhow::anyhow!("unknown preset {raw:?}; expected generic|xr18|dl32s|studio")
                })?;
            }
            v if v.starts_with("--preset=") => {
                let raw = &v["--preset=".len()..];
                preset = openfader_sim::SimPreset::parse(raw).ok_or_else(|| {
                    anyhow::anyhow!("unknown preset {raw:?}; expected generic|xr18|dl32s|studio")
                })?;
            }
            "-h" | "--help" | "help" => {
                print!("{HELP}");
                return Ok(());
            }
            other => rest.push(other.to_string()),
        }
        i += 1;
    }

    if !["sim", "xair", "dl32s"].contains(&driver.as_str()) {
        anyhow::bail!("unknown driver {driver:?}; expected sim|xair|dl32s");
    }
    if rest.is_empty() {
        print!("{HELP}");
        std::process::exit(2);
    }

    // `discover` needs no driver instance.
    if rest[0] == "discover" {
        let mut timeout = Duration::from_secs(2);
        if let Some(pos) = rest.iter().position(|a| a == "--timeout") {
            let ms: u64 = rest
                .get(pos + 1)
                .ok_or_else(|| anyhow::anyhow!("--timeout needs MS"))?
                .parse()?;
            timeout = Duration::from_millis(ms);
        }
        let found = openfader_transport::discover_xair(timeout).await?;
        if found.is_empty() {
            println!("no X-Air devices found");
        }
        for d in &found {
            println!("{} {} {}", d.addr, d.driver, d.model);
        }
        return Ok(());
    }

    let drv = make_driver(&driver, preset);
    let target = addr.clone().unwrap_or_else(|| {
        if driver == "sim" {
            "sim://local".to_string()
        } else {
            "192.168.1.1".to_string()
        }
    });
    if let Err(e) = drv.connect(&target).await {
        // Hardware stubs bail until their protocol lands; report cleanly.
        eprintln!("connect {driver} @ {target} failed: {e:#}");
        std::process::exit(1);
    }
    let count = drv.channel_count();

    match rest[0].as_str() {
        "list" => {
            // Per-strip queries (fader/on/name reads), never `snapshot()`:
            // the xair driver has no snapshot impl, and this way `list`
            // works against live hardware (T6). One row per channel strip.
            for i in 0..count {
                let strip = StripId(i as u16);
                let name = drv.get_name(strip).await?;
                let fader = drv.get_fader(strip).await?;
                let mute = drv.get_mute(strip).await?;
                println!(
                    "{:>3} {:<8} fader={:.2} {}",
                    // `s.id.0 + 1` display numbering kept from snapshot form.
                    strip.0 + 1,
                    name,
                    fader,
                    if mute { "MUTED" } else { "      " }
                );
            }
        }
        "fader" => {
            let strip = parse_ch(
                rest.get(1)
                    .ok_or_else(|| anyhow::anyhow!("usage: fader <ch> <0-100>"))?,
                count,
            )?;
            let pct: f32 = rest
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("usage: fader <ch> <0-100>"))?
                .parse()
                .map_err(|_| anyhow::anyhow!("level must be 0..=100"))?;
            if !(0.0..=100.0).contains(&pct) {
                anyhow::bail!("level must be 0..=100");
            }
            drv.set_fader(strip, pct / 100.0).await?;
            println!("ch{} fader -> {pct:.0}", strip.0 + 1);
        }
        "mute" => {
            let strip = parse_ch(
                rest.get(1)
                    .ok_or_else(|| anyhow::anyhow!("usage: mute <ch> <on|off>"))?,
                count,
            )?;
            let state = rest
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("usage: mute <ch> <on|off>"))?;
            let muted = match state.to_lowercase().as_str() {
                "on" | "1" | "true" | "mute" => true,
                "off" | "0" | "false" | "unmute" => false,
                _ => anyhow::bail!("mute state must be on|off"),
            };
            drv.set_mute(strip, muted).await?;
            println!(
                "ch{} {}",
                strip.0 + 1,
                if muted { "muted" } else { "unmuted" }
            );
        }
        "name" => {
            let strip = parse_ch(
                rest.get(1)
                    .ok_or_else(|| anyhow::anyhow!("usage: name <ch> <name>"))?,
                count,
            )?;
            let name = rest
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("usage: name <ch> <name>"))?;
            drv.set_name(strip, name).await?;
            println!("ch{} name -> {name}", strip.0 + 1);
        }
        "param" => {
            let strip = parse_ch(
                rest.get(1)
                    .ok_or_else(|| anyhow::anyhow!("usage: param <ch> <id> [value]"))?,
                count,
            )?;
            let id = rest
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("usage: param <ch> <id> [value]"))?;
            match rest.get(3) {
                None => match drv.get_param(strip, id).await? {
                    ParamValue::Float(v) => println!("ch{} {id} = {v}", strip.0 + 1),
                    ParamValue::Bool(b) => println!(
                        "ch{} {id} = {}",
                        strip.0 + 1,
                        if b { "on" } else { "off" }
                    ),
                    ParamValue::Text(s) => println!("ch{} {id} = {s:?}", strip.0 + 1),
                },
                Some(raw) => {
                    let value = parse_param_value(id, raw)?;
                    drv.set_param(strip, id, value).await?;
                    println!("ch{} {id} -> {raw}", strip.0 + 1);
                }
            }
        }
        "meters" => {
            let frame = drv.poll_meters();
            let shown: Vec<String> =
                frame.levels.iter().take(8).map(|l| format!("{l:.2}")).collect();
            println!("meters[{}]: {}", frame.levels.len(), shown.join(" "));
        }
        other => {
            eprintln!("unknown command {other:?}\n{HELP}");
            std::process::exit(2);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_value_typing_follows_catalogue() {
        // Float/enum ids take numbers (send tap is an enum → float).
        assert!(matches!(
            parse_param_value("pan", "25").unwrap(),
            ParamValue::Float(_)
        ));
        assert!(matches!(
            parse_param_value("send1_tap", "2").unwrap(),
            ParamValue::Float(_)
        ));
        // Bool ids take on/off words.
        assert_eq!(
            parse_param_value("mute", "on").unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            parse_param_value("phantom", "off").unwrap(),
            ParamValue::Bool(false)
        );
        // Text ids keep the raw string.
        assert_eq!(
            parse_param_value("name", "TEST").unwrap(),
            ParamValue::Text("TEST".to_string())
        );
        // Wrong shapes fail before touching the desk.
        assert!(parse_param_value("pan", "abc").is_err());
        assert!(parse_param_value("mute", "maybe").is_err());
    }

    #[test]
    fn param_value_unknown_id_falls_back() {
        // Ids this catalogue does not name (future desk leaves) still
        // parse: number → bool-word → string, so the CLI can drive them on
        // hardware that answers without a per-id subcommand.
        assert!(matches!(
            parse_param_value("not_a_real_param", "1").unwrap(),
            ParamValue::Float(_)
        ));
        assert_eq!(
            parse_param_value("not_a_real_param", "on").unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            parse_param_value("not_a_real_param", "OffX").unwrap(),
            ParamValue::Text("OffX".to_string())
        );
    }

    #[test]
    fn param_value_new_xair_ids_follow_catalogue() {
        // v2b ids (lr_assign, sendN_grpon, automix_weight, fx_insert):
        // typed by the catalogue, so the generic `param` command drives
        // them with no per-id CLI work.
        assert_eq!(
            parse_param_value("lr_assign", "1").unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            parse_param_value("send1_grpon", "off").unwrap(),
            ParamValue::Bool(false)
        );
        assert!(matches!(
            parse_param_value("automix_weight", "0.5").unwrap(),
            ParamValue::Float(_)
        ));
        assert_eq!(
            parse_param_value("fx_insert", "on").unwrap(),
            ParamValue::Bool(true)
        );
    }
}
