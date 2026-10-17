# openmix over the network: `openmix-serve` + Docker

`openmix-serve` is the run-anywhere head of openmix. One process serves the
static `ui/` directory over HTTP **and** a WebSocket mixer API on one port
(default `8080`; `--port`, `--bind`):

- `GET /` — the mixer UI
- `GET /healthz` — `{"ok": true, "driver": "sim"}`
- `WS /ws` — the mixer protocol (channels are 0-based, as in `openmix-core`):

client → server:

```json
{"cmd":"connect","driver":"sim|xair|dl32s","addr":"..."}
{"cmd":"fader","ch":0,"value":0.5}
{"cmd":"mute","ch":0,"muted":true}
{"cmd":"param","ch":0,"field":"gain","value":0.5}
```

server → client:

```json
{"type":"state","driver":"sim","strips":[{"ch":0,"name":"CH01","fader":0.7,"muted":false}]}
{"type":"meters","levels":[0.1,0.2]}
```

`state` is pushed on connect and after every change; `meters` ticks at
~10 Hz. Bad commands get `{"type":"error","message":"..."}` without
dropping the connection.

## Why browsers need this bridge

Mixers speak UDP on the LAN (OSC for X-Air, TCP frames for the DL32S).
A web page cannot open a UDP socket, so the browser cannot talk to the
mixer directly. `openmix-serve` runs on a machine that **can** reach the
mixer and translates: browser ↔ WebSocket ↔ `openmix-serve` ↔ UDP/TCP ↔
mixer. The Tauri desktop app does the same translation in-process; the
server is the equivalent for browsers, phones, and containers.

## Run locally

```sh
# sim driver (default): UI + fake mixer on http://localhost:8080
cargo run -p openmix-serve

# hardware on the LAN (server must reach the mixer address)
cargo run -p openmix-serve -- --driver xair --addr 192.168.1.10 --port 8080
```

Then open `http://<server-lan-ip>:8080` from a laptop/phone browser on the
same network and press connect (or send `{"cmd":"connect",...}` over
`/ws`). `--bind` defaults to `0.0.0.0` so LAN devices can reach it;
use `--bind 127.0.0.1` to keep it loopback-only.

The UI directory is auto-detected (`ui/`, `$OPENMIX_UI_DIR`,
`--static-dir`); without one the server still runs the API.

## Run with Docker

```sh
docker build -t openmix .
docker run --rm -p 8080:8080 openmix
# studio desk preset (sim driver):
docker run -d -p 8080:8080 openmix --preset studio
# hardware: point the server at the mixer (same LAN / routable net)
docker run --rm -p 8080:8080 openmix \
  --driver xair --addr 192.168.1.10
```

Open `http://<host-lan-ip>:8080` from any browser on the network.

### Compose (sim + scenes volume)

```sh
docker compose up --build        # sim on http://localhost:8080
docker compose up --build -d     # detached (e.g. a Pi at the venue)
# hardware via .env (DRIVER=xair|dl32s, ADDR=<mixer-ip>):
DRIVER=xair ADDR=192.168.1.10 docker compose up --build
```

`docker-compose.yml` mounts `./scenes:/app/scenes` — an operator-managed
directory for saved scene/snapshot JSON. The server is stateless; the
volume keeps saved scenes next to the deployment instead of inside the
container layer.

### Image notes (verified 2026-10-17)

- Multi-stage build: `rust:1-bookworm` builder compiles
  `cargo build --release -p openmix-serve`, runtime is
  `debian:bookworm-slim` running as non-root `openmix`.
- `ENTRYPOINT ["openmix-serve"]` with `CMD ["--driver", "sim"]`, so
  extra `docker run` args append to the server flags
  (`docker run openmix --preset studio`). `docker-compose.yml`
  overrides the entrypoint with an equivalent `sh` wrapper to keep
  `DRIVER`/`ADDR` `.env` support.
- `HEALTHCHECK` polls `GET /healthz` via `wget` (the only addition to
  `-slim`; no curl/python in the base).
- The dependency tree is pure Rust (no OpenSSL in `Cargo.lock`), so the
  `-slim` runtime needs no extra system libraries.
- `.dockerignore` excludes `target/` (2.7 GB in a dev checkout), `.git/`,
  `src-tauri/` and `apps/` — the build context is only
  `Cargo.toml` + `Cargo.lock` + `crates/` + `ui/`.
  (The runtime stage `COPY ui ./ui` needs `ui/` in context — do not ignore it.)
- Verified: `docker build -t openmix .` succeeds; image size **120 MB**
  (118 MB before the `wget` HEALTHCHECK dependency).
- Base pin: `rust:1-bookworm` (major-version pin + suite match with the
  runtime). Renovate/dependabot can bump the patch; re-verify with the
  build command above after any base change.

### Raspberry Pi / aarch64

- Both stages are Debian bookworm, which publishes `arm64` variants, so
  the same `Dockerfile` builds natively on a Pi:
  `docker build -t openmix .` on Raspberry Pi OS (64-bit) / Ubuntu 22.04+.
- Use a **Pi 4 (4 GB+) or Pi 5**. The release build peaks around 2–4 GB
  RAM; on a 4 GB Pi add swap (`2 GB`) or build with fewer codegen units.
  Expect ~10–20 min for the first (cold-cache) build on a Pi 5.
- Cross-building from x86: `docker buildx build --platform linux/arm64
  -t openmix:arm64 .` (needs QEMU `binfmt`; slow — prefer native Pi builds).
- At the venue run detached: `docker compose up --build -d`, then open
  `http://<pi-lan-ip>:8080` from a phone/laptop. For X-Air hardware on the
  same subnet you may need `--network host` (Linux only) if you rely on
  UDP broadcast discovery instead of `--addr <mixer-ip>`.

Notes:

- The image runs as a non-root `openmix` user and defaults to
  `openmix-serve --driver sim`.
- X-Air discovery (UDP broadcast) does not cross Docker's default bridge
  network — pass the mixer IP explicitly with `--addr`. If you need
  broadcast discovery inside the container, run with `--network host`
  (Linux only).
- Hardware UDP ports must be reachable **from the container/host running
  the server**, not from the browser — that is the whole point of the
  bridge.
