# openfader-serve: headless run-anywhere server (HTTP static UI + WebSocket mixer API).
#
# Build:  docker build -t openfader-serve .
# Run:    docker run --rm -p 8080:8080 openfader-serve
#         docker run --rm -p 8080:8080 -v ./ui:/ui:ro -e OPENFADER_UI_DIR=/ui openfader-serve
#         docker run --rm -p 8080:8080 -v openfader-data:/data openfader-serve --data-dir /data
#
# The image carries both binaries:
#   openfader-serve  HTTP+WS relay on one port (default 8080, `--bind 0.0.0.0`).
#   openfader-cli    headless mixer CLI for container debugging.
# Browsers cannot speak UDP/OSC, so hardware mixers (X-Air, DL32S) need this
# bridge on the LAN: browser -> WebSocket -> openfader-serve -> mixer UDP.

FROM rust:1-bookworm AS builder
WORKDIR /src

# Dependency manifests first for layer caching.
COPY Cargo.toml Cargo.lock VERSION ./
COPY crates ./crates

# Release binaries only (no Flutter SDK needed: serve/cli are pure Rust).
RUN cargo build --locked --release -p openfader-serve -p openfader-cli

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /usr/sbin/nologin openfader

COPY --from=builder /src/target/release/openfader-serve /usr/local/bin/openfader-serve
COPY --from=builder /src/target/release/openfader-cli /usr/local/bin/openfader-cli

# Optional static web UI: mount at /ui (ro) or set OPENFADER_UI_DIR.
# Absent -> API-only with a helpful pointer at `/` (WebSocket API at `/ws`).
# Optional scene library: mount a volume at /data and pass `--data-dir /data`.
RUN mkdir -p /ui /data && chown openfader:openfader /ui /data
VOLUME ["/data"]

USER openfader
WORKDIR /app
EXPOSE 8080
ENV PORT=8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT:-8080}/healthz" > /dev/null || exit 1

# `${PORT}`-aware default (PaaS convention): `docker run -e PORT=9000 -p 9000:9000`.
# Extra `docker run` args append as flags (e.g. `--driver xair --addr 192.168.1.10`).
ENTRYPOINT ["sh", "-c", "exec openfader-serve --bind 0.0.0.0 --port \"${PORT:-8080}\" \"$@\"", "--"]
