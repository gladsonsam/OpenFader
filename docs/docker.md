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
# hardware: point the server at the mixer (same LAN / routable net)
docker run --rm -p 8080:8080 openmix \
  openmix-serve --driver xair --addr 192.168.1.10
```

Open `http://<host-lan-ip>:8080` from any browser on the network.

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
