FROM rust:1.96-trixie AS base

RUN apt-get update && apt-get install -y \
    curl \
    xz-utils \
    git \
    binaryen \
    fonts-dejavu \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

ARG UID=1000
ARG GID=1000

RUN groupadd -g ${GID} dev && \
    useradd -m -u ${UID} -g ${GID} -s /bin/bash dev

USER dev
ENV USER=dev

# Add WASM target for Dioxus frontend
RUN rustup target add wasm32-unknown-unknown

# Persist bash history to a mountable directory
RUN mkdir -p /home/dev/.bash_history_dir && \
    echo 'export HISTFILE=/home/dev/.bash_history_dir/.bash_history' >> /home/dev/.bashrc

WORKDIR /app

COPY bootstrap.sh bootstrap.sh
RUN ./bootstrap.sh

# Dev target: adds sqlx-cli for local development (slow to compile, not needed for builds)
FROM base AS dev
RUN cargo install sqlx-cli

FROM base AS build
USER root
RUN apt-get update && apt-get install -y nodejs npm && rm -rf /var/lib/apt/lists/* && npm install -g esbuild
USER dev
# Must match the wasm-bindgen version in Cargo.lock — `dx bundle` (with
# NO_DOWNLOADS=1) refuses to run on a mismatch. --locked for the armv7
# source-build fallback (no prebuilt binary; see bootstrap.sh).
RUN cargo binstall -y --locked wasm-bindgen-cli@0.2.126
COPY . /app
RUN --mount=type=cache,target=/app/target,uid=1000,gid=1000 \
    --mount=type=cache,target=/home/dev/.cargo/registry,uid=1000,gid=1000 \
    NO_DOWNLOADS=1 dx bundle --release --debug-symbols false && \
    cp -r /app/dist /home/dev/dist-output && \
    cargo build --features server --message-format=short --color never --release && \
    cp target/release/srvr /home/dev/dist-output/server

FROM debian:trixie-slim AS publish
RUN apt-get update && apt-get install -y \
    ca-certificates \ 
    fonts-dejavu \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /data/

ARG UID=1000
ARG GID=1000
RUN groupadd -g ${GID} dev && \
    useradd -m -u ${UID} -g ${GID} -s /bin/bash dev

RUN chown dev:dev /data

USER dev
ENV USER=dev

COPY --from=build /home/dev/dist-output /dist

WORKDIR /

ENV IP=0.0.0.0
ENV PORT=8080

CMD ["/dist/server"]

# Headless Chromium exposed as a WebDriver endpoint for the browser E2E tests
# (tests/browser_e2e.rs). Kept out of the dev image so no browser ships there;
# the docker-compose `chrome` service builds this stage.
FROM debian:trixie-slim AS chrome
RUN apt-get update && apt-get install -y \
    chromium \
    chromium-driver \
    fonts-dejavu \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 4444
# --allowed-ips= (empty) accepts connections from any IP on the compose network;
# --allowed-origins=* permits fantoccini's requests. chromedriver launches
# headless Chromium per session using the args the tests pass.
CMD ["chromedriver", "--port=4444", "--allowed-ips=", "--allowed-origins=*"]
