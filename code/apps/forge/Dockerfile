# Forge in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml), from code/ so it
# can use the studio's kit (code/kit/sync).

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/forge
COPY apps/forge/package.json apps/forge/bun.lock apps/forge/tsconfig.base.json ./
COPY apps/forge/apps/web/package.json apps/web/
COPY apps/forge/apps/desktop/package.json apps/desktop/
COPY apps/forge/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY apps/forge/packages packages
COPY apps/forge/apps/web apps/web
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/forge
COPY apps/forge/Cargo.toml apps/forge/Cargo.lock ./
COPY apps/forge/crates crates
COPY apps/forge/migrations migrations
COPY kit/sync /code/kit/sync
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/forge/apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/forge/target \
    cargo build --release -p forge-server && cp target/release/forge-server /usr/local/bin/forge-server

FROM debian:trixie-slim
# git: drift checks read repositories with it.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git && rm -rf /var/lib/apt/lists/* \
    && git config --system --add safe.directory '*'
COPY --from=server /usr/local/bin/forge-server /usr/local/bin/forge-server
COPY --from=web /code/apps/forge/apps/web/dist /srv/forge/web
ENV FORGE_ADDR=0.0.0.0:7380 \
    FORGE_WEB_DIR=/srv/forge/web \
    FORGE_DATA_DIR=/var/lib/forge
VOLUME /var/lib/forge
EXPOSE 7380
CMD ["forge-server"]
