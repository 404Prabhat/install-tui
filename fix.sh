#!/usr/bin/env bash
set -euo pipefail

DEFAULT_TARGET="$HOME/arch_dev_setup.sh"

usage() {
    cat <<'EOF'
Usage: ./fix.sh [--all] [TARGET...]

Validates required fix markers in one or more setup scripts.
If no TARGET is provided, defaults to ~/arch_dev_setup.sh.
EOF
}

validate_target() {
    local target="$1"

    if [[ ! -f "$target" ]]; then
        echo "Target script not found: $target" >&2
        exit 1
    fi

    local -a required_patterns=(
        'OPT_YES=0'
        'OPT_GPU_CHOICE=""'
        '_print_log_header()'
        '_sudo_keepalive()'
        'read -r -t 30 ans || ans="$default"'
        'read -r -t 30 NVIDIA_CHOICE || true'
        '_heartbeat_start "running pacman -Syu"'
        '_heartbeat_start "pip installing ML libraries"'
        'if command -v pnpm &>/dev/null; then'
    )

    if ! grep -Eq 'SCRIPT_VERSION="[0-9]+\.[0-9]+\.[0-9]+"' "$target"; then
        echo 'Missing expected fix marker: SCRIPT_VERSION="<semver>"' >&2
        exit 1
    fi

    if ! grep -Fq '_heartbeat_start "installing PyTorch (large download)"' "$target" &&
       ! grep -Fq '_heartbeat_start "installing PyTorch (large download — can take 10+ min)"' "$target"; then
        echo 'Missing expected fix marker: _heartbeat_start "installing PyTorch (large download...)"' >&2
        exit 1
    fi

    for pattern in "${required_patterns[@]}"; do
        if ! grep -Fq "$pattern" "$target"; then
            echo "Missing expected fix marker: $pattern" >&2
            exit 1
        fi
    done

    bash -n "$target"
}

targets=()
if [[ $# -eq 0 ]]; then
    targets=("$DEFAULT_TARGET")
else
    for arg in "$@"; do
        case "$arg" in
            --help|-h)
                usage
                exit 0
                ;;
            --all)
                targets+=("$DEFAULT_TARGET")
                ;;
            --*)
                echo "Unknown option: $arg" >&2
                usage >&2
                exit 1
                ;;
            *)
                targets+=("$arg")
                ;;
        esac
    done
fi

if [[ ${#targets[@]} -eq 0 ]]; then
    targets=("$DEFAULT_TARGET")
fi

for target in "${targets[@]}"; do
    validate_target "$target"
    echo "Fix verification passed: $target"
done
