#!/bin/sh
# Builds the guests' page (the app for the browser) into web/pkg; the desktop app bundles it.
# Needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version 0.2.129
set -e
cd "$(dirname "$0")/.."
cargo build --profile web --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg --out-name tratto target/wasm32-unknown-unknown/web/tratto.wasm
ls -l web/pkg
