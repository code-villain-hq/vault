# Forge in one image: the Rust server, serving the built web app.
# Built and run by `bun run app` (see docker-compose.yml), from code/ so it
# can use the studio's kit (code/kit).

FROM oven/bun:1.4 AS web
WORKDIR /code/apps/forge
COPY apps/forge/package.json apps/forge/bun.lock apps/forge/tsconfig.base.json ./
COPY apps/forge/apps/web/package.json apps/web/
COPY apps/forge/apps/desktop/package.json apps/desktop/
COPY apps/forge/packages/types/package.json packages/types/
COPY kit/sync/client/package.json /code/kit/sync/client/
COPY kit/web/package.json /code/kit/web/
RUN bun install --frozen-lockfile
COPY kit/sync/client /code/kit/sync/client
COPY kit/web /code/kit/web
COPY apps/forge/packages packages
COPY apps/forge/apps/web apps/web
# The web app shows the version Cargo.toml keeps.
COPY apps/forge/Cargo.toml Cargo.toml
RUN bun run --cwd apps/web build

FROM rust:1-slim-trixie AS server
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /code/apps/forge
COPY apps/forge/Cargo.toml apps/forge/Cargo.lock ./
COPY apps/forge/crates crates
COPY apps/forge/migrations migrations
COPY kit/sync /code/kit/sync
COPY kit/desktop /code/kit/desktop
COPY kit/auth /code/kit/auth
COPY kit/claude /code/kit/claude
COPY kit/scim /code/kit/scim
COPY kit/settings /code/kit/settings
COPY kit/notify /code/kit/notify
# Cargo needs every workspace member's manifest, even when not building it.
COPY apps/forge/apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/code/apps/forge/target \
    cargo build --release -p forge-server && cp target/release/forge-server /usr/local/bin/forge-server

FROM debian:trixie-slim
# git: drift checks read repositories with it. Forge's own clones (under
# /var/lib/forge/repos) belong to the user that runs it, so git trusts them;
# repositories mounted under /repos (local_repos_root) belong to whoever owns
# them on the host, so git is told to trust that folder, and only that one.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git && rm -rf /var/lib/apt/lists/* \
    && git config --system --add safe.directory '/repos/*'
# The server runs as code-villain (10001:10001, the same in every Code Villain
# image), which owns only what the server writes: the data folder.
RUN groupadd --gid 10001 code-villain \
    && useradd --uid 10001 --gid 10001 --no-create-home --home-dir /nonexistent \
       --shell /usr/sbin/nologin code-villain \
    && install -d -o 10001 -g 10001 -m 0750 /var/lib/forge
COPY --from=server /usr/local/bin/forge-server /usr/local/bin/forge-server
COPY --from=web /code/apps/forge/apps/web/dist /srv/forge/web
ENV FORGE_ADDR=0.0.0.0:7380 \
    FORGE_WEB_DIR=/srv/forge/web \
    FORGE_DATA_DIR=/var/lib/forge
VOLUME /var/lib/forge
USER 10001:10001
EXPOSE 7380
CMD ["forge-server"]
