#!/bin/bash

set -e 

mkdir .data/

cargo install dioxus-cli
rustup component add rustfmt
