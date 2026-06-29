#!/bin/bash

set -e

mkdir .data/

# Install cargo-binstall for fast prebuilt binary installs
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

cargo binstall -y dioxus-cli@0.7.9
# wasm-bindgen-test-runner drives the browser-tier frontend tests; the version
# must match the wasm-bindgen pinned in Cargo.lock. A headless browser
# (chromium + chromium-driver) is installed at the system level in the Dockerfile.
cargo binstall -y wasm-bindgen-cli@0.2.115
rustup component add rustfmt
