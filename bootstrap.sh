#!/bin/bash

set -e

mkdir .data/

# Install cargo-binstall for fast prebuilt binary installs
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

cargo binstall -y dioxus-cli@0.7.9
# cargo binstall -y cargo-watch
rustup component add rustfmt
