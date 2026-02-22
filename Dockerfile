
FROM rust:1.93-trixie AS dev

RUN apt-get update && apt-get install -y \
    curl \
    xz-utils \
    git \
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


FROM dev AS build
COPY . /app
RUN dx bundle --release

FROM debian:trixie-slim AS publish
RUN apt-get update && apt-get install -y \
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

COPY --from=build /app/dist /dist

WORKDIR /dist

ENV IP=0.0.0.0
ENV PORT=8080

CMD ["/dist/srvr"]