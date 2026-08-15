#!/usr/bin/env bash
# UniProgrammer full lint check (Linux/macOS)
set -euo pipefail

cd "$(dirname "$0")"

npm run lint
npm run format:check

cd src-tauri
cargo fmt --check
cargo lint
