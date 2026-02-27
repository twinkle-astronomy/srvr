#!/bin/bash

set -e 

mkdir .data/

cargo install dioxus-cli
cargo install sqlx-cli
rustup component add rustfmt
