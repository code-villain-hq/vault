# Glyph in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml), from code/ so it
# can use the studio's kit (code/kit/sync).

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/glyph
COPY apps/glyph/package.json apps/glyph/bun.lock apps/glyph/bunfig.toml apps/glyph/tsconfig.base.json ./
COPY apps/glyph/apps/web/package.json apps/web/
COPY apps/glyph/apps/desktop/package.json apps/desktop/
COPY apps/glyph/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY apps/glyph/packages packages
COPY apps/glyph/apps/web apps/web
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/glyph
COPY apps/glyph/Cargo.toml apps/glyph/Cargo.lock ./
COPY apps/glyph/crates crates
COPY apps/glyph/migrations migrations
COPY kit/sync /code/kit/sync
COPY kit/desktop /code/kit/desktop
COPY kit/scim /code/kit/scim
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/glyph/apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/glyph/target \
    cargo build --release -p glyph-server && cp target/release/glyph-server /usr/local/bin/glyph-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/glyph-server /usr/local/bin/glyph-server
COPY --from=web /code/apps/glyph/apps/web/dist /srv/glyph/web
ENV GLYPH_ADDR=0.0.0.0:7480 \
    GLYPH_WEB_DIR=/srv/glyph/web
EXPOSE 7480
CMD ["glyph-server"]
