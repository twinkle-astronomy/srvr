#!/bin/bash

set -e

mkdir .data/

# Install cargo-binstall for fast prebuilt binary installs
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# --locked matters on armv7: no prebuilt dioxus-cli exists there, so binstall
# falls back to `cargo install`, and an unlocked resolve picks transitive deps
# that don't compile together (e.g. auth-git2 vs a newer git2).
cargo binstall -y --locked dioxus-cli@0.7.9
# cargo binstall -y cargo-watch
rustup component add rustfmt
