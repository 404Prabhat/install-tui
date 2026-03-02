#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/arch-package-tui "$HOME/.local/bin/arch-package-tui"

echo "Installed: $HOME/.local/bin/arch-package-tui"
echo "Run with: arch-package-tui"
