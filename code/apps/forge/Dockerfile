# Forge in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml).

FROM oven/bun:1.4 AS web
WORKDIR /forge
COPY package.json bun.lock tsconfig.base.json ./
COPY apps/web/package.json apps/web/
COPY apps/desktop/package.json apps/desktop/
COPY packages/sync-client/package.json packages/sync-client/
COPY packages/types/package.json packages/types/
RUN bun install --frozen-lockfile
COPY packages packages
COPY apps/web apps/web
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /forge
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY migrations migrations
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/forge/target \
    cargo build --release -p forge-server && cp target/release/forge-server /usr/local/bin/forge-server

FROM debian:trixie-slim
# git: drift checks read repositories with it.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git && rm -rf /var/lib/apt/lists/* \
    && git config --system --add safe.directory '*'
COPY --from=server /usr/local/bin/forge-server /usr/local/bin/forge-server
COPY --from=web /forge/apps/web/dist /srv/forge/web
ENV FORGE_ADDR=0.0.0.0:7380 \
    FORGE_WEB_DIR=/srv/forge/web \
    FORGE_DATA_DIR=/var/lib/forge
VOLUME /var/lib/forge
EXPOSE 7380
CMD ["forge-server"]
