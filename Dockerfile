# openmix run-anywhere image: static UI + WebSocket mixer API on one port.
# The UI worker owns ui/; this image just serves whatever is in ui/ as built.
FROM rust:1-bookworm AS builder
WORKDIR /build

# Workspace manifest + lockfile first for a cacheable dependency layer.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p openmix-serve

FROM debian:bookworm-slim
RUN useradd --create-home --shell /usr/sbin/nologin openmix
WORKDIR /app
COPY --from=builder /build/target/release/openmix-serve /usr/local/bin/openmix-serve
# Static UI served by openmix-serve (override with --static-dir / $OPENMIX_UI_DIR).
COPY ui ./ui
RUN chown -R openmix:openmix /app
USER openmix
EXPOSE 8080
CMD ["openmix-serve", "--driver", "sim"]
