# syntax=docker/dockerfile:1

# ---- 1. Build the web client -------------------------------------------------
FROM node:22-alpine AS web
WORKDIR /build
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ---- 2. Build the Rust server ------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /build
# Compile the dependency graph first so code changes don't rebuild every crate.
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY server/src ./src
COPY server/migrations ./migrations
# Cargo skips a rebuild when mtimes look unchanged after the dummy main.rs.
RUN touch src/main.rs && cargo build --release --locked

# ---- 3. Runtime --------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 minichat

COPY --from=server /build/target/release/minichat-server /usr/local/bin/minichat-server
COPY --from=web /build/dist /srv/web

ENV MINICHAT_DATA_DIR=/data \
    WEB_DIR=/srv/web \
    BIND_ADDR=0.0.0.0:8080

RUN mkdir -p /data && chown -R minichat:minichat /data
VOLUME ["/data"]
USER minichat
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

CMD ["minichat-server"]
