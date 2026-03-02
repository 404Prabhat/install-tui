#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
TARGET_BIN="${ROOT_DIR}/target/release/arch-package-tui"
INSTALL_BIN="${BIN_DIR}/arch-package-tui"
ALIAS_BIN="${BIN_DIR}/install-tui"

AUTO_RUN=1
if [[ "${1:-}" == "--no-run" ]]; then
  AUTO_RUN=0
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required but not found in PATH." >&2
  exit 1
fi

echo "Building release binary..."
cargo build --release --manifest-path "${ROOT_DIR}/Cargo.toml"

mkdir -p "${BIN_DIR}"
install -m 755 "${TARGET_BIN}" "${INSTALL_BIN}"
ln -sf "${INSTALL_BIN}" "${ALIAS_BIN}"

echo ""
echo "Installed commands:"
echo "  arch-package-tui"
echo "  install-tui"
echo ""
echo "Run from anywhere:"
echo "  install-tui"

if (( AUTO_RUN == 1 )); then
  if [[ -t 0 && -t 1 ]]; then
    exec "${ALIAS_BIN}"
  else
    echo ""
    echo "Non-interactive shell detected, skipping auto-launch."
    echo "Launch manually with: install-tui"
  fi
fi
