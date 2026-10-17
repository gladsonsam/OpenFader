//! openmix-serve: headless run-anywhere server.
//!
//! Serves the static `ui/` directory over HTTP plus a WebSocket mixer API
//! on one port, backed by any `openmix-core` driver (sim default).
//!
//! Browsers cannot speak UDP/OSC, so hardware mixers (X-Air, DL32S) need
//! this bridge on the LAN: the browser talks WebSocket to `openmix-serve`,
//! which translates to mixer UDP on the server side.

use std::{net::SocketAddr, path::PathBuf};

use openmix_serve::{run, ServeOptions};

const HELP: &str = "\
Usage: openmix-serve [--port PORT] [--bind ADDR] [--driver sim|xair|dl32s]
                     [--addr IP] [--static-dir DIR]

Options:
  --port <n>       HTTP+WS port (default 8080)
  --bind <addr>    bind address (default 0.0.0.0, reachable over LAN)
  --driver <name>  sim (default), xair, dl32s
  --addr <ip>      mixer address (xair/dl32s; sim ignores it)
  --static-dir <d> UI directory (default: auto-detect ui/)
  -h, --help       print this help
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut port: u16 = 8080;
    let mut bind = "0.0.0.0".to_string();
    let mut opts = ServeOptions::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
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
                opts.static_dir = Some(PathBuf::from(
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("--static-dir needs a value"))?,
                ));
            }
            v if v.starts_with("--static-dir=") => {
                opts.static_dir = Some(PathBuf::from(&v["--static-dir=".len()..]));
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
    run(listener, opts).await
}
