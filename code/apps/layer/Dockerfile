# Layer's server in one image: the Rust server, serving the built web app.
# Built and run by `docker compose --profile app up --build` (see
# docker-compose.yml), from code/ so it can use the studio's kit (code/kit).
# The desktop app isn't in it.

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/layer
COPY apps/layer/package.json apps/layer/bun.lock ./
COPY apps/layer/web/package.json web/
COPY apps/layer/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
COPY kit/web/package.json /code/kit/web/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY kit/web /code/kit/web
COPY apps/layer/packages packages
COPY apps/layer/web web
# The web app shows the version the desktop app's manifest keeps.
COPY apps/layer/src-tauri/Cargo.toml src-tauri/Cargo.toml
RUN bun run --cwd web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/layer
COPY apps/layer/Cargo.toml apps/layer/Cargo.lock ./
COPY apps/layer/crates crates
COPY apps/layer/migrations migrations
COPY kit/sync /code/kit/sync
COPY kit/desktop /code/kit/desktop
COPY kit/scim /code/kit/scim
COPY kit/settings /code/kit/settings
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/layer/src-tauri src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/layer/target \
    cargo build --release -p layer-server && cp target/release/layer-server /usr/local/bin/layer-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/layer-server /usr/local/bin/layer-server
COPY --from=web /code/apps/layer/dist-web /srv/layer/web
ENV LAYER_ADDR=0.0.0.0:7680 \
    LAYER_WEB_DIR=/srv/layer/web
EXPOSE 7680
CMD ["layer-server"]
