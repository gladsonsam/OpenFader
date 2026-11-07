# OpenFader server and the same Flutter web app shipped by the desktop builds.
#
# Build: docker build -t openfader-serve .
# Run:   docker run --rm -p 8080:8080 -p 8845:8845 -v openfader-data:/data openfader-serve
# Mixer: append --driver <registered-driver> --addr <mixer-address>.
# Optional shared password: OPENFADER_SERVER_PASSWORD (persisted under /data).

FROM debian:bookworm-slim AS web-builder
ARG FLUTTER_VERSION=3.47.7
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates curl git unzip xz-utils zip libglu1-mesa \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 --branch "${FLUTTER_VERSION}" \
      https://github.com/flutter/flutter.git /opt/flutter
ENV PATH="/opt/flutter/bin:${PATH}"
RUN flutter config --no-analytics --enable-web \
    && flutter precache --web
WORKDIR /src/app
COPY app/pubspec.yaml app/pubspec.lock ./
RUN flutter pub get --enforce-lockfile
COPY app ./
RUN flutter build web --release --no-pub --no-web-resources-cdn

FROM rust:1-bookworm AS rust-builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libasound2-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock VERSION ./
COPY crates ./crates
RUN cargo build --locked --release \
      -p openfader-serve -p openfader-cli -p openfader-audio --bins

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libasound2 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --user-group --create-home \
       --shell /usr/sbin/nologin openfader \
    && mkdir -p /data /usr/local/share/openfader/web \
    && chown openfader:openfader /data

COPY --from=rust-builder /src/target/release/openfader-serve /usr/local/bin/openfader-serve
COPY --from=rust-builder /src/target/release/openfader-cli /usr/local/bin/openfader-cli
COPY --from=rust-builder /src/target/release/openfader-capture-helper /usr/local/bin/openfader-capture-helper
COPY --from=web-builder /src/app/build/web/ /usr/local/share/openfader/web/
COPY LICENSE /usr/local/share/openfader/LICENSE

ENV PORT=8080 \
    APP_PORT=8845 \
    OPENFADER_DATA_DIR=/data \
    OPENFADER_WEB_DIR=/usr/local/share/openfader/web
VOLUME ["/data"]
USER openfader
WORKDIR /data
EXPOSE 8080 8845

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT:-8080}/web/connect" > /dev/null || exit 1

# The public port uses the authenticated browser host. Administration stays on
# an ephemeral loopback port. Native apps use their existing TLS transport on
# APP_PORT with the same password and engine. Extra args preserve driver selection.
ENTRYPOINT ["sh", "-c", "exec openfader-serve --bind 127.0.0.1 --port 0 --app-bind 0.0.0.0 --app-port \"${APP_PORT:-8845}\" --web-bind 0.0.0.0 --web-port \"${PORT:-8080}\" --web-dir \"${OPENFADER_WEB_DIR}\" --data-dir \"${OPENFADER_DATA_DIR}\" \"$@\"", "--"]
