# syntax=docker/dockerfile:1

# The host and its console in one image, with the opensocial command: the
# compose file in deploy/ runs it once as each of the host and the console
# (docs/deploying.md).
#
#   docker build --build-arg GROUP_HOST_COMMIT=$(git rev-parse HEAD) -t group-host:current .

# The toolchain CI tests with (.knotless/workflows/test.yml).
FROM rust:1.98-bookworm AS host
WORKDIR /src
COPY . .
# The cache mounts keep the registry and the build between builds on the
# same machine, so a change rebuilds the crates it touches and not the
# rest. GROUP_HOST_COMMIT is the commit the host names in its health and
# as it starts: only group-host reads it, so it rebuilds nothing else.
ARG GROUP_HOST_COMMIT
ENV GROUP_HOST_COMMIT=${GROUP_HOST_COMMIT}
# How many jobs cargo runs, where the build shares a machine that serves
# something else; unset, every core.
ARG CARGO_BUILD_JOBS
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --profile deploy --locked \
        -p group-host --bin group-host \
        -p opensocial --bin opensocial \
    && cp target/deploy/group-host target/deploy/opensocial /

# The console, its types made from the lexicons beside it, and the page
# the host serves around a site that asks for live data (web/shell),
# which builds on the SDK beside it (packages/sdk, its types made from the
# same lexicons).
FROM node:24.22.1-bookworm-slim AS console
WORKDIR /packages/sdk
COPY packages/sdk/package.json packages/sdk/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY packages/sdk/ ./
COPY lexicons/ /lexicons/
RUN npm run lex
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# adapter-node bundles what the console imports, so it needs no
# node_modules beside it; the host's healthcheck asks with node's fetch.
FROM node:24.22.1-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=host /group-host /opensocial /usr/local/bin/
COPY --from=console /web/build /app/build
COPY --from=console /web/build-shell /app/build-shell
COPY --from=console /web/package.json /app/package.json
# The host's state, a volume the compose file mounts, owned by the node
# user (uid 1000) the image runs as; and where it finds the site shell.
ENV DATA_DIR=/data PORT=2600 SITE_SHELL_DIR=/app/build-shell
RUN mkdir /data && chown node:node /data
USER node
WORKDIR /app
ENTRYPOINT ["tini", "--"]
CMD ["group-host"]
