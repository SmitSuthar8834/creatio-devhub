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

# Make sure Rust is on PATH even if this shell wasn't restarted after rustup
# installed it. rustup appends the env line to your shell profile, but an
# already-open Terminal won't have picked it up yet — the classic
# "cargo metadata ... No such file or directory" failure.
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: 'cargo' not found — Rust is not installed." >&2
  echo "  Install it:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
  echo "  Then:        source \"\$HOME/.cargo/env\" && ./dev.sh" >&2
  exit 1
fi

npm install
npm run tauri dev
