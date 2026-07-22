#!/usr/bin/env bash
# Launch Creatio DevHub in dev mode on macOS / Linux.
#
# Unlike Windows (dev.cmd, which shims the VS BuildTools MSVC toolchain), the
# Unix toolchains work out of the box, so this is a thin wrapper. Dev mode does
# NOT build updater artifacts, so no signing key is needed to run and test.
#
# Prerequisites on the tester's Mac: Node 22+, Rust (rustup), and the CLIs
# DevHub drives — clio (`dotnet tool install -g clio`), git, and gh — on PATH.
set -euo pipefail
cd "$(dirname "$0")"
npm install
npm run tauri dev
