# openmix-serve: headless run-anywhere server (HTTP static UI + WebSocket mixer API).
#
# Build:  docker build -t openmix-serve .
# Run:    docker run --rm -p 8080:8080 openmix-serve
#         docker run --rm -p 8080:8080 -v ./ui:/ui:ro -e OPENMIX_UI_DIR=/ui openmix-serve
#         docker run --rm -p 8080:8080 -v openmix-data:/data openmix-serve --data-dir /data
#
# The image carries both binaries:
#   openmix-serve  HTTP+WS relay on one port (default 8080, `--bind 0.0.0.0`).
#   openmix-cli    headless mixer CLI for container debugging.
# Browsers cannot speak UDP/OSC, so hardware mixers (X-Air, DL32S) need this
# bridge on the LAN: browser -> WebSocket -> openmix-serve -> mixer UDP.

FROM rust:1-bookworm AS builder
WORKDIR /src

# Dependency manifests first for layer caching.
COPY Cargo.toml Cargo.lock VERSION ./
COPY crates ./crates

# Release binaries only (no Flutter SDK needed: serve/cli are pure Rust).
RUN cargo build --locked --release -p openmix-serve -p openmix-cli

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /usr/sbin/nologin openmix

COPY --from=builder /src/target/release/openmix-serve /usr/local/bin/openmix-serve
COPY --from=builder /src/target/release/openmix-cli /usr/local/bin/openmix-cli

# Optional static web UI: mount at /ui (ro) or set OPENMIX_UI_DIR.
# Absent -> API-only with a helpful pointer at `/` (WebSocket API at `/ws`).
# Optional scene library: mount a volume at /data and pass `--data-dir /data`.
RUN mkdir -p /ui /data && chown openmix:openmix /ui /data
VOLUME ["/data"]

USER openmix
WORKDIR /app
EXPOSE 8080
ENV PORT=8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT:-8080}/healthz" > /dev/null || exit 1

# `${PORT}`-aware default (PaaS convention): `docker run -e PORT=9000 -p 9000:9000`.
# Extra `docker run` args append as flags (e.g. `--driver xair --addr 192.168.1.10`).
ENTRYPOINT ["sh", "-c", "exec openmix-serve --bind 0.0.0.0 --port \"${PORT:-8080}\" \"$@\"", "--"]
