#!/usr/bin/env bash
# Build a production DevHub bundle on macOS (universal .app/.dmg) or Linux.
#
# Signed release builds are normally CI's job (.github/workflows/release.yml).
# This is for a local production build. The updater key is optional here: if
# ~/.tauri/creatio-devhub.key exists it is used, otherwise the build still
# produces the app but its updater artifacts are unsigned (fine for local test,
# NOT for a release users update from).
set -euo pipefail
cd "$(dirname "$0")"

key="$HOME/.tauri/creatio-devhub.key"
if [[ -f "$key" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$key"
fi

npm install
if [[ "$(uname)" == "Darwin" ]]; then
  # One bundle that runs on both Apple Silicon and Intel.
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  npm run tauri build -- --target universal-apple-darwin
else
  npm run tauri build
fi
