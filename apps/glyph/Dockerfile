# Glyph in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml).

FROM oven/bun:1.4 AS web
WORKDIR /glyph
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
WORKDIR /glyph
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY migrations migrations
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/glyph/target \
    cargo build --release -p glyph-server && cp target/release/glyph-server /usr/local/bin/glyph-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/glyph-server /usr/local/bin/glyph-server
COPY --from=web /glyph/apps/web/dist /srv/glyph/web
ENV GLYPH_ADDR=0.0.0.0:7480 \
    GLYPH_WEB_DIR=/srv/glyph/web
EXPOSE 7480
CMD ["glyph-server"]
