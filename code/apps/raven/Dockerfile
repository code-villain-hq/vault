# Raven in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml), from code/ so it
# can use the studio's kit (code/kit).

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/raven
COPY apps/raven/package.json apps/raven/bun.lock apps/raven/tsconfig.base.json ./
COPY apps/raven/apps/web/package.json apps/web/
COPY apps/raven/apps/desktop/package.json apps/desktop/
COPY apps/raven/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
COPY kit/web/package.json /code/kit/web/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY kit/web /code/kit/web
COPY apps/raven/packages packages
COPY apps/raven/apps/web apps/web
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/raven
COPY apps/raven/Cargo.toml apps/raven/Cargo.lock ./
COPY apps/raven/crates crates
COPY apps/raven/migrations migrations
COPY kit/sync /code/kit/sync
COPY kit/desktop /code/kit/desktop
COPY kit/scim /code/kit/scim
COPY kit/settings /code/kit/settings
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/raven/apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/raven/target \
    cargo build --release -p raven-server && cp target/release/raven-server /usr/local/bin/raven-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/raven-server /usr/local/bin/raven-server
COPY --from=web /code/apps/raven/apps/web/dist /srv/raven/web
ENV RAVEN_ADDR=0.0.0.0:7580 \
    RAVEN_WEB_DIR=/srv/raven/web
EXPOSE 7580
CMD ["raven-server"]
