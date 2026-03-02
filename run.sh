#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if [[ -x "./target/release/arch-package-tui" ]]; then
  exec ./target/release/arch-package-tui "$@"
fi

exec cargo run -- "$@"
