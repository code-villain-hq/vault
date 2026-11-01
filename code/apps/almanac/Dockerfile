# Almanac in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml), from code/ so it
# can use the studio's kit (code/kit).

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/almanac
COPY apps/almanac/package.json apps/almanac/bun.lock apps/almanac/tsconfig.base.json ./
COPY apps/almanac/apps/web/package.json apps/web/
COPY apps/almanac/apps/desktop/package.json apps/desktop/
COPY apps/almanac/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
COPY kit/web/package.json /code/kit/web/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY kit/web /code/kit/web
COPY apps/almanac/packages packages
COPY apps/almanac/apps/web apps/web
# The web app shows the version Cargo.toml keeps.
COPY apps/almanac/Cargo.toml Cargo.toml
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/almanac
COPY apps/almanac/Cargo.toml apps/almanac/Cargo.lock ./
COPY apps/almanac/crates crates
COPY apps/almanac/migrations migrations
COPY kit/sync /code/kit/sync
COPY kit/desktop /code/kit/desktop
COPY kit/auth /code/kit/auth
COPY kit/scim /code/kit/scim
COPY kit/settings /code/kit/settings
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/almanac/apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/almanac/target \
    cargo build --release -p almanac-server && cp target/release/almanac-server /usr/local/bin/almanac-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/almanac-server /usr/local/bin/almanac-server
COPY --from=web /code/apps/almanac/apps/web/dist /srv/almanac/web
ENV ALMANAC_ADDR=0.0.0.0:7780 \
    ALMANAC_WEB_DIR=/srv/almanac/web
EXPOSE 7780
CMD ["almanac-server"]
