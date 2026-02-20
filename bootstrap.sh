#!/bin/bash

set -e

rustup target add wasm32-unknown-unknown
rustup component add rustfmt

cargo install cargo-leptos --locked
cargo install diesel_cli --no-default-features --features sqlite
cargo install cargo-watch
