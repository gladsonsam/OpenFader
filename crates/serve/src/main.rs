//! openfader-serve: headless run-anywhere server.
//!
//! Serves the static `ui/` directory over HTTP plus a WebSocket mixer API
//! on one port, backed by any `openfader-core` driver (sim default).
//!
//! Browsers cannot speak UDP/OSC, so hardware mixers (X-Air, DL32S) need
//! this bridge on the LAN: the browser talks WebSocket to `openfader-serve`,
//! which translates to mixer UDP on the server side.

use std::{net::SocketAddr, path::PathBuf};

use openfader_serve::ServeOptions;

const HELP: &str = "\
Usage: openfader-serve [--port PORT] [--bind ADDR] [--driver sim|xair|dl32s]
                     [--preset generic|xr18|dl32s|studio] [--addr IP] [--static-dir DIR]
                     [--data-dir DIR] [--poll-inflight N]

Options:
  --sim-only      Disable hardware connections and LAN discovery
  --port <n>       HTTP+WS port (default 8080)
  --bind <addr>    bind address (default 0.0.0.0, reachable over LAN)
  --driver <name>  sim (default), xair, dl32s
  --preset <name>  sim desk preset: generic (default), xr18, dl32s, studio
                   (hardware drivers ignore it)
  --addr <ip>      mixer address (xair/dl32s; sim ignores it)
  --static-dir <d> UI directory (default: auto-detect ui/)
  --data-dir <d>   scene library directory (R2: persisted scenes.json;
                   default: in-memory scenes only)
  --poll-ms <ms>   param-plane poll tick in ms (default 50 = 20 queries/s);
                   lower values speed up change detection for tests
  --poll-inflight <n> R2 poller pipelining: concurrent driver queries per
                   tick (default: sim 4, xair 1 until rig-verified)
  -h, --help       print this help
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut port: u16 = 8080;
    let mut bind = "0.0.0.0".to_string();
    let mut poll_ms: u64 = 50;
    let mut opts = ServeOptions::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--sim-only" => opts.sim_only = true,
            "--port" => {
                i += 1;
                port = argv
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("--port needs a value"))?
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--port must be a number"))?;
            }
            v if v.starts_with("--port=") => {
                port = v["--port=".len()..]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--port must be a number"))?;
            }
            "--bind" => {
                i += 1;
                bind = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--bind needs a value"))?;
            }
            v if v.starts_with("--bind=") => {
                bind = v["--bind=".len()..].to_string();
            }
            "--driver" => {
                i += 1;
                opts.driver = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--driver needs a value"))?;
            }
            v if v.starts_with("--driver=") => {
                opts.driver = v["--driver=".len()..].to_string();
            }
            "--preset" => {
                i += 1;
                opts.preset = Some(
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--preset needs a value"))?,
                );
            }
            v if v.starts_with("--preset=") => {
                opts.preset = Some(v["--preset=".len()..].to_string());
            }
            "--addr" => {
                i += 1;
                opts.addr = Some(
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--addr needs a value"))?,
                );
            }
            v if v.starts_with("--addr=") => {
                opts.addr = Some(v["--addr=".len()..].to_string());
            }
            "--static-dir" => {
                i += 1;
                opts.static_dir =
                    Some(PathBuf::from(argv.get(i).cloned().ok_or_else(|| {
                        anyhow::anyhow!("--static-dir needs a value")
                    })?));
            }
            v if v.starts_with("--static-dir=") => {
                opts.static_dir = Some(PathBuf::from(&v["--static-dir=".len()..]));
            }
            "--poll-ms" => {
                i += 1;
                poll_ms = argv
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("--poll-ms needs a value"))?
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--poll-ms must be a number"))?;
            }
            v if v.starts_with("--poll-ms=") => {
                poll_ms = v["--poll-ms=".len()..]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--poll-ms must be a number"))?;
            }
            "--poll-inflight" => {
                i += 1;
                opts.poll_inflight = Some(
                    argv.get(i)
                        .ok_or_else(|| anyhow::anyhow!("--poll-inflight needs a value"))?
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--poll-inflight must be a number"))?,
                );
            }
            v if v.starts_with("--poll-inflight=") => {
                opts.poll_inflight = Some(
                    v["--poll-inflight=".len()..]
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--poll-inflight must be a number"))?,
                );
            }
            "--data-dir" => {
                i += 1;
                opts.data_dir =
                    Some(PathBuf::from(argv.get(i).cloned().ok_or_else(|| {
                        anyhow::anyhow!("--data-dir needs a value")
                    })?));
            }
            v if v.starts_with("--data-dir=") => {
                opts.data_dir = Some(PathBuf::from(&v["--data-dir=".len()..]));
            }
            "-h" | "--help" | "help" => {
                print!("{HELP}");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}\n{HELP}"),
        }
        i += 1;
    }

    let sock: SocketAddr = format!("{bind}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(sock).await?;
    openfader_serve::run_with_full(
        listener,
        opts,
        openfader_serve::LOCK_TTL,
        std::time::Duration::from_millis(poll_ms),
        None,
        None,
    )
    .await
}
