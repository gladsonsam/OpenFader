# openmix run-anywhere image: static UI + WebSocket mixer API on one port.
# The UI worker owns ui/; this image just serves whatever is in ui/ as built.
FROM rust:1-bookworm AS builder
WORKDIR /build

# Workspace manifest + lockfile first for a cacheable dependency layer.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p openmix-serve

FROM debian:bookworm-slim
# wget only for the /healthz HEALTHCHECK below (no curl/python in -slim).
RUN apt-get update && apt-get install -y --no-install-recommends wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /usr/sbin/nologin openmix
WORKDIR /app
COPY --from=builder /build/target/release/openmix-serve /usr/local/bin/openmix-serve
# Static UI served by openmix-serve (override with --static-dir / $OPENMIX_UI_DIR).
COPY ui ./ui
RUN chown -R openmix:openmix /app
USER openmix
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://127.0.0.1:8080/healthz | grep -q '"ok"'
# ENTRYPOINT so `docker run openmix <flags>` appends to openmix-serve
# (without it, args replace CMD and `--preset` is exec'd as a binary).
ENTRYPOINT ["openmix-serve"]
CMD ["--driver", "sim"]
