#!/bin/bash

set -e 

rustup target add wasm32-unknown-unknown
rustup component add rustfmt

curl -sSL https://dioxus.dev/install.sh | bash
