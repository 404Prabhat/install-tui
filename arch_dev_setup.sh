#!/usr/bin/env bash
# ╔══════════════════════════════════════════════════════════════════════════════╗
# ║            ARCH LINUX  ·  ELITE DEVELOPMENT ENVIRONMENT SETUP               ║
# ║                              v4.0.0                                          ║
# ╠══════════════════════════════════════════════════════════════════════════════╣
# ║  Modular · Parallel · Idempotent · Instrumented · Resumable                 ║
# ╠══════════════════════════════════════════════════════════════════════════════╣
# ║  MODES        --minimal  --dev  --ml  --full                                 ║
# ║  FLAGS        --fast  --dry-run  --resume  --no-reflector                   ║
# ║               --phase <name>  --list-phases  --benchmark  --help            ║
# ╠══════════════════════════════════════════════════════════════════════════════╣
# ║  PHASES       preflight · mirrors · base · gpu · fonts · cli                ║
# ║               sysutils · version_managers · python · rust                   ║
# ║               node · dev_tools · ml_stack · shell · neovim                 ║
# ╠══════════════════════════════════════════════════════════════════════════════╣
# ║  EXAMPLES                                                                    ║
# ║    bash arch_dev_setup.sh --dev --fast                                       ║
# ║    bash arch_dev_setup.sh --ml --fast --resume                               ║
# ║    bash arch_dev_setup.sh --phase rust --dry-run                             ║
# ║    bash arch_dev_setup.sh --dev --fast --no-reflector --resume               ║
# ╚══════════════════════════════════════════════════════════════════════════════╝
#
# WINNER RATIONALE (duplicates removed — one tool per job):
#   ls    → eza        over lsd    — git integration, better defaults
#   cat   → bat        over delta  — syntax + pager in one
#   find  → fd         over locate — faster, .gitignore-aware, sane syntax
#   grep  → ripgrep    over ag     — fastest, always recursion, PCRE2
#   cd    → zoxide     over autojump — frecency, Rust, no Python dep
#   diff  → delta      over vimdiff  — git-native, syntax highlight
#   top   → btop       over htop   — GPU support, mouse, modern TUI
#   df    → duf        over df     — visual, color, device grouping
#   du    → dust       over ncdu   — instant, Rust, no deps
#   ps    → procs      over ps     — color, search, tree
#   man   → tealdeer   over tldr   — Rust, offline, instant
#   mux   → zellij     over tmux   — config-as-code, modern UX
#   prompt→ starship   over p10k   — universal, zero-config, fast
#   hist  → atuin      over mcfly  — encrypted, syncable, SQL search
#   node  → fnm        over nvm    — Rust, 40× faster, no shell overhead
#   vm    → mise       over asdf   — Rust, 10× faster, replaces all vm tools
#   pip   → uv         over pip    — Rust, 10-100× faster, venv-in-one
# ──────────────────────────────────────────────────────────────────────────────

# ══════════════════════════════════════════════════════════════════════════════
#  STRICT MODE — errors are fatal; unset variables fatal; pipe failures fatal
# ══════════════════════════════════════════════════════════════════════════════
# -e  errexit:  exit immediately if any command exits non-zero, unless it is
#               part of an if/while/until condition, or followed by || or &&.
# -u  nounset:  treat unset variables as errors — catches typos like
#               $NVIDIA_CHIOCE (silent zero-length string without -u).
# -o pipefail:  if any command in a pipe fails, the whole pipe fails.
#               Without this, `false | true` exits 0 (masks the failure).
set -euo pipefail

# Remove space from IFS (Internal Field Separator).
# Default IFS splits on space, tab, and newline. Keeping only newline+tab
# prevents accidental word-splitting on unquoted variables that may contain
# spaces (e.g. $HOME paths), while still allowing newline-based iteration.
IFS=$'\n\t'

# ══════════════════════════════════════════════════════════════════════════════
#  CONSTANTS
# ══════════════════════════════════════════════════════════════════════════════
readonly SCRIPT_VERSION="4.0.0"
readonly SCRIPT_NAME="$(basename "${BASH_SOURCE[0]}")"
readonly SCRIPT_PID=$$

readonly BASE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/arch-setup"
readonly LOG_FILE="${BASE_DIR}/setup-$(date +%Y%m%d-%H%M%S).log"
readonly STATE_FILE="${BASE_DIR}/state"
readonly LOCK_FILE="${BASE_DIR}/setup.lock"
readonly BENCH_FILE="${BASE_DIR}/benchmarks.json"
readonly FAILED_LOG="${BASE_DIR}/failed.log"
readonly MIRROR_CACHE="${BASE_DIR}/mirrorlist.cache"
readonly MIRROR_CACHE_TTL=86400   # 24 h

readonly MAX_PARALLEL_AUR=4       # concurrent AUR background jobs
readonly PACMAN_PARALLEL_DL=10    # ParallelDownloads value
readonly CURL_TIMEOUT=30          # seconds for curl health checks
readonly RETRY_ATTEMPTS=2         # retry count for transient failures

# ══════════════════════════════════════════════════════════════════════════════
#  ANSI — gracefully degrade when not a TTY
# ══════════════════════════════════════════════════════════════════════════════
_init_colors() {
    if [[ -t 1 && "${TERM:-}" != "dumb" ]]; then
        RED='\033[0;31m'    GREEN='\033[0;32m'   YELLOW='\033[1;33m'
        BLUE='\033[0;34m'   CYAN='\033[0;36m'    MAGENTA='\033[0;35m'
        WHITE='\033[1;37m'  DIM='\033[2m'         BOLD='\033[1m'
        RESET='\033[0m'     ERASE='\033[2K'       CR='\r'
        # Cursor control
        HIDE_CURSOR='\033[?25l'
        SHOW_CURSOR='\033[?25h'
    else
        RED='' GREEN='' YELLOW='' BLUE='' CYAN='' MAGENTA=''
        WHITE='' DIM='' BOLD='' RESET='' ERASE='' CR=''
        HIDE_CURSOR='' SHOW_CURSOR=''
    fi
}
_init_colors

# ══════════════════════════════════════════════════════════════════════════════
#  RUNTIME STATE
# ══════════════════════════════════════════════════════════════════════════════
MODE=""
OPT_FAST=0
OPT_DRY=0
OPT_RESUME=0
OPT_NO_REFLECTOR=0
OPT_BENCHMARK=0
OPT_LIST_PHASES=0
OPT_PHASE=""
OPT_YES=0            # --yes / -y   → auto-answer all prompts (fully unattended)
OPT_GPU_CHOICE=""    # --gpu-choice → pre-select NVIDIA driver at launch time
NVIDIA_CHOICE=""

declare -A  PHASE_TIMES=()
declare -a  FAILED_PKGS=()
declare -a  FAILED_REMEDIATION=()
declare -a  SKIPPED_PKGS=()
declare -a  BG_PIDS=()          # tracked background jobs — killed in _cleanup()
declare -i  INSTALL_COUNT=0
declare -i  CURRENT_PHASE=0
declare -i  TOTAL_PHASES=0
declare -i  SCRIPT_START=$SECONDS
declare -i  NET_RX_START=0
declare -i  NET_TX_START=0
SPINNER_PID=""
HEARTBEAT_PID=""

# ══════════════════════════════════════════════════════════════════════════════
#  LOGGING — All output tee'd to log. Log uses plain text (no ANSI).
# ══════════════════════════════════════════════════════════════════════════════
mkdir -p "${BASE_DIR}"
exec > >(tee -a "${LOG_FILE}") 2>&1

# ══════════════════════════════════════════════════════════════════════════════
#  LOG HEADER — printed immediately on startup, before any work begins.
#  The user needs the log path BEFORE the script potentially hangs on a prompt.
#  This also appears in the log itself (tee is active by the time we call it).
# ══════════════════════════════════════════════════════════════════════════════
_print_log_header() {
    printf "\n${BOLD}${CYAN}  ┌──────────────────────────────────────────────────────┐${RESET}\n"
    printf "${BOLD}${CYAN}  │  arch-dev-setup v%-4s  started: %-20s│${RESET}\n" \
        "$SCRIPT_VERSION" "$(date '+%Y-%m-%d %H:%M:%S')"
    printf "${BOLD}${CYAN}  │  📋 Log: ${YELLOW}%-44s${CYAN} │${RESET}\n" "${LOG_FILE}"
    printf "${BOLD}${CYAN}  │  Monitor: ${YELLOW}tail -f %-39s${CYAN} │${RESET}\n" "${LOG_FILE}"
    printf "${BOLD}${CYAN}  └──────────────────────────────────────────────────────┘${RESET}\n\n"
}

# ══════════════════════════════════════════════════════════════════════════════
#  UI — output helpers (concise, not verbose)
# ══════════════════════════════════════════════════════════════════════════════
_ok()       { printf "${GREEN}  ✔${RESET}  %s\n"       "$*"; }
_fail()     { printf "${RED}  ✘${RESET}  %s\n"         "$*" >&2; }
_info()     { printf "${CYAN}  →${RESET}  %s\n"        "$*"; }
_warn()     { printf "${YELLOW}  ⚠${RESET}  %s\n"      "$*"; }
_skip()     { printf "${DIM}  ⊘  skip: %s${RESET}\n"   "$*"; }
_dim()      { printf "${DIM}     %s${RESET}\n"          "$*"; }
_section()  { printf "\n${BOLD}${BLUE}  ▸ %s${RESET}\n" "$*"; }
_fatal()    { _fail "$*"; _cleanup; exit 1; }

_elapsed() {
    local s=$1
    printf "%dm%02ds" "$(( s / 60 ))" "$(( s % 60 ))"
}

_phase_banner() {
    local name="$1"
    CURRENT_PHASE=$(( CURRENT_PHASE + 1 ))

    # Guard against division by zero and clamp pct to [0,100] in case
    # CURRENT_PHASE overshoots TOTAL_PHASES (miscounting protection).
    local pct=0
    if (( TOTAL_PHASES > 0 )); then
        pct=$(( CURRENT_PHASE * 100 / TOTAL_PHASES ))
        (( pct > 100 )) && pct=100
    fi

    # Bar is exactly 20 chars wide. Clamp filled to [0,20] to prevent loop overrun.
    local filled=$(( pct / 5 ))
    (( filled > 20 )) && filled=20
    (( filled < 0  )) && filled=0

    local bar="" i
    for ((i=0; i<filled; i++));    do bar+="█"; done
    for ((i=filled; i<20; i++)); do bar+="░"; done

    # ETA: avg time-per-completed-phase × remaining phases
    local elapsed=$(( SECONDS - SCRIPT_START ))
    local eta_str=""
    if (( CURRENT_PHASE > 1 && elapsed > 0 )); then
        local secs_per=$(( elapsed / (CURRENT_PHASE - 1) ))
        local remaining=$(( (TOTAL_PHASES - CURRENT_PHASE) * secs_per ))
        eta_str="  ETA ~$(_elapsed ${remaining})"
    fi

    printf "\n${BOLD}${MAGENTA}  ══ PHASE %d/%d  %-30s${RESET}\n" \
        "$CURRENT_PHASE" "$TOTAL_PHASES" "$name"
    printf "  ${CYAN}[%s] %d%%  elapsed: %s%s${RESET}\n\n" \
        "$bar" "$pct" "$(_elapsed ${elapsed})" "$eta_str"
}

# ══════════════════════════════════════════════════════════════════════════════
#  SPINNER  — runs in background; killed by _spin_stop
# ══════════════════════════════════════════════════════════════════════════════
_spin_start() {
    # Spinner MUST write to /dev/tty directly. After `exec > >(tee ...)`,
    # stdout is a pipe — [[ -t 1 ]] is always false and the spinner never starts.
    # Writing to /dev/tty bypasses tee: visible on-screen but never in the log.
    [[ -e /dev/tty ]] || return 0
    local msg="${1:-Working…}"
    (
        exec > /dev/tty 2>&1     # subshell writes only to the real terminal
        local frames=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')
        local i=0
        printf "${HIDE_CURSOR}"
        while true; do
            printf "${ERASE}${CR}${CYAN}  %s${RESET}  %s" "${frames[$i]}" "$msg"
            i=$(( (i + 1) % ${#frames[@]} ))
            sleep 0.08
        done
    ) &
    SPINNER_PID=$!
    disown "$SPINNER_PID" 2>/dev/null || true
}

_spin_stop() {
    [[ -z "$SPINNER_PID" ]] && return 0
    kill "$SPINNER_PID" 2>/dev/null || true
    wait "$SPINNER_PID" 2>/dev/null || true
    SPINNER_PID=""
    # Clear spinner line + restore cursor directly on terminal, not in log
    [[ -e /dev/tty ]] && printf "${ERASE}${CR}${SHOW_CURSOR}" > /dev/tty || true
}

# ══════════════════════════════════════════════════════════════════════════════
#  HEARTBEAT LOGGER
#  Long silent ops (pacman -Syu, PyTorch download, cargo compile) go quiet for
#  5-20 minutes. Without a heartbeat there is no way to tell "working" from
#  "frozen" when tailing the log. The heartbeat writes only to the log file
#  (not stdout/terminal) so it never interferes with the spinner or output.
# ══════════════════════════════════════════════════════════════════════════════
_heartbeat_start() {
    local msg="${1:-working}"
    (
        local elapsed=0
        while true; do
            sleep 30
            elapsed=$(( elapsed + 30 ))
            printf "  [heartbeat %s] still %s — %ds elapsed\n" \
                "$(date '+%H:%M:%S')" "$msg" "$elapsed" >> "${LOG_FILE}"
        done
    ) &
    HEARTBEAT_PID=$!
    disown "$HEARTBEAT_PID" 2>/dev/null || true
}

_heartbeat_stop() {
    [[ -z "$HEARTBEAT_PID" ]] && return 0
    kill "$HEARTBEAT_PID" 2>/dev/null || true
    wait "$HEARTBEAT_PID" 2>/dev/null || true
    HEARTBEAT_PID=""
}
# ══════════════════════════════════════════════════════════════════════════════
_acquire_lock() {
    if [[ -f "${LOCK_FILE}" ]]; then
        local other_pid
        other_pid=$(cat "${LOCK_FILE}" 2>/dev/null || echo "unknown")
        if kill -0 "$other_pid" 2>/dev/null; then
            _fatal "Another instance is running (PID $other_pid). Remove ${LOCK_FILE} to force."
        else
            _warn "Stale lock file found — removing"
            rm -f "${LOCK_FILE}"
        fi
    fi
    echo "${SCRIPT_PID}" > "${LOCK_FILE}"
}

_release_lock() {
    [[ -f "${LOCK_FILE}" ]] && rm -f "${LOCK_FILE}"
}

# ══════════════════════════════════════════════════════════════════════════════
#  SIGNAL HANDLING + CLEANUP
# ══════════════════════════════════════════════════════════════════════════════
_cleanup() {
    local exit_code=${1:-$?}
    _spin_stop
    _heartbeat_stop

    # Kill any tracked background jobs
    for pid in "${BG_PIDS[@]:-}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true

    _release_lock

    printf "${SHOW_CURSOR}"

    if [[ $exit_code -ne 0 ]]; then
        printf "\n${RED}  Script exited with code %d${RESET}\n" "$exit_code"
        printf "${DIM}  Log: %s${RESET}\n" "${LOG_FILE}"
        printf "${DIM}  Retry with --resume to continue from last completed phase.${RESET}\n\n"
    fi
}

trap '_cleanup $?' EXIT
trap '_spin_stop; printf "\n${YELLOW}  Interrupted.${RESET}\n"; exit 130' INT TERM QUIT

# ══════════════════════════════════════════════════════════════════════════════
#  STATE FILE  (resume support)
# ══════════════════════════════════════════════════════════════════════════════
_state_done()  { echo "$1" >> "${STATE_FILE}"; }
_state_check() { grep -qxF "$1" "${STATE_FILE}" 2>/dev/null; }
_state_clear() { rm -f "${STATE_FILE}"; _ok "State cleared — full reinstall on next run"; }

_phase_skip_if_done() {
    local name="$1"
    if [[ $OPT_RESUME -eq 1 ]] && _state_check "$name"; then
        _skip "phase '${name}' (completed — use without --resume to re-run)"
        CURRENT_PHASE=$(( CURRENT_PHASE + 1 ))
        return 0
    fi
    return 1
}

# ══════════════════════════════════════════════════════════════════════════════
#  DRY-RUN WRAPPER
# ══════════════════════════════════════════════════════════════════════════════
X() {
    if [[ $OPT_DRY -eq 1 ]]; then
        _dim "[dry] $*"
        return 0
    fi
    "$@"
}

# ══════════════════════════════════════════════════════════════════════════════
#  NETWORK BYTES (for instrumentation)
# ══════════════════════════════════════════════════════════════════════════════
_net_bytes() {
    # Sum RX bytes across all non-loopback interfaces
    awk '
        /^\s*(eth|en|wl|ww|ppp)/ {
            gsub(/:/, " ")
            rx += $2; tx += $10
        }
        END { printf "%d %d", rx, tx }
    ' /proc/net/dev 2>/dev/null || echo "0 0"
}

_net_snapshot() {
    read -r NET_RX_START NET_TX_START <<< "$(_net_bytes)"
}

_net_delta() {
    local rx tx
    read -r rx tx <<< "$(_net_bytes)"
    local drx=$(( (rx - NET_RX_START) / 1024 / 1024 ))
    local dtx=$(( (tx - NET_TX_START) / 1024 / 1024 ))
    printf "↓ %dMB  ↑ %dMB" "$drx" "$dtx"
}

# ══════════════════════════════════════════════════════════════════════════════
#  CORE INSTALL PRIMITIVES
# ══════════════════════════════════════════════════════════════════════════════

# ── pacman_batch ──────────────────────────────────────────────────────────────
#  Single pacman -S call for all missing packages in a group.
#  This is the primary speed gain vs. per-package calls.
pacman_batch() {
    local label="$1"; shift
    local -a pkgs=("$@") missing=()

    for p in "${pkgs[@]}"; do
        pacman -Qi "$p" &>/dev/null || missing+=("$p")
    done

    if [[ ${#missing[@]} -eq 0 ]]; then
        _skip "${label} (all present)"
        SKIPPED_PKGS+=("${label}")
        return 0
    fi

    _info "${label}: installing ${#missing[@]} package(s)…"
    local attempt
    for attempt in $(seq 1 $RETRY_ATTEMPTS); do
        if X sudo pacman -S --noconfirm --needed "${missing[@]}"; then
            INSTALL_COUNT=$(( INSTALL_COUNT + ${#missing[@]} ))
            _ok "${label}"
            return 0
        fi
        (( attempt < RETRY_ATTEMPTS )) && { _warn "Retry ${attempt}/${RETRY_ATTEMPTS} for ${label}…"; sleep 3; }
    done

    _fail "${label} — pacman batch failed"
    FAILED_PKGS+=("pacman:${label}")
    FAILED_REMEDIATION+=("  sudo pacman -S --needed ${missing[*]}")
}

# ── paru_one ──────────────────────────────────────────────────────────────────
paru_one() {
    local pkg="$1"
    command -v paru &>/dev/null || { FAILED_PKGS+=("aur:${pkg} (paru missing)"); return 0; }
    { paru -Qi "$pkg" &>/dev/null || pacman -Qi "$pkg" &>/dev/null; } \
        && { SKIPPED_PKGS+=("$pkg"); return 0; }

    local attempt
    for attempt in $(seq 1 $RETRY_ATTEMPTS); do
        if X paru -S --noconfirm --needed --skipreview "$pkg"; then
            INSTALL_COUNT=$(( INSTALL_COUNT + 1 ))
            _ok "  $pkg"
            return 0
        fi
        (( attempt < RETRY_ATTEMPTS )) && sleep 3
    done

    _fail "  $pkg (AUR)"
    FAILED_PKGS+=("aur:${pkg}")
    FAILED_REMEDIATION+=("  paru -S ${pkg}")
}

# ── paru_batch ────────────────────────────────────────────────────────────────
#  Sequential or parallel (--fast) AUR installs.
paru_batch() {
    local label="$1"; shift
    local -a pkgs=("$@")
    _section "AUR: ${label}"

    if [[ $OPT_FAST -eq 1 ]]; then
        local -a pids=() pkg
        for pkg in "${pkgs[@]}"; do
            paru_one "$pkg" &
            pids+=($!)
            BG_PIDS+=($!)
            if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
                # Save before slicing: after pids=("${pids[@]:1}"),
                # pids[0] is already the NEXT element — wrong PID to remove.
                local finished_pid="${pids[0]}"
                wait "${finished_pid}" 2>/dev/null || true
                pids=("${pids[@]:1}")
                BG_PIDS=("${BG_PIDS[@]/${finished_pid}}")
            fi
        done
        wait "${pids[@]}" 2>/dev/null || true
    else
        for pkg in "${pkgs[@]}"; do paru_one "$pkg"; done
    fi
}

# ── curl_install ──────────────────────────────────────────────────────────────
#  Idempotent check: command availability is authoritative. Directory fallback
#  is for tools that don't immediately appear on PATH after install (e.g. sdkman).
#  Previously the broad directory check caused false-positives for tools like
#  uv/mise/fnm that don't install to a predictable dotfolder.
curl_install() {
    local label="$1" check_cmd="$2" url="$3"
    shift 3
    local extra_args=("$@")

    if command -v "$check_cmd" &>/dev/null; then
        _skip "${label} ($(${check_cmd} --version 2>/dev/null | head -1 || echo 'installed'))"
        SKIPPED_PKGS+=("${label}")
        return 0
    elif [[ -d "${HOME}/.${check_cmd}" ]] || [[ -d "${HOME}/${check_cmd}" ]]; then
        _skip "${label} (install dir exists — may need shell reload to appear on PATH)"
        SKIPPED_PKGS+=("${label}")
        return 0
    fi

    _info "Installing ${label}…"
    if X bash -c "curl -fsSL '${url}' | bash ${extra_args[*]:-}"; then
        INSTALL_COUNT=$(( INSTALL_COUNT + 1 ))
        _ok "${label}"
    else
        _fail "${label}"
        FAILED_PKGS+=("curl:${label}")
        FAILED_REMEDIATION+=("  curl -fsSL ${url} | bash ${extra_args[*]:-}")
    fi
}

# ── pipx_one ──────────────────────────────────────────────────────────────────
pipx_one() {
    local pkg="$1"
    command -v pipx &>/dev/null || return 0
    pipx list 2>/dev/null | grep -qF "$pkg" && { SKIPPED_PKGS+=("$pkg"); return 0; }
    X pipx install "$pkg" \
        && { INSTALL_COUNT=$(( INSTALL_COUNT + 1 )); _ok "  $pkg (pipx)"; } \
        || { FAILED_PKGS+=("pipx:${pkg}"); FAILED_REMEDIATION+=("  pipx install ${pkg}"); }
}

# ── npm_global ────────────────────────────────────────────────────────────────
npm_global() {
    local pkg="$1"
    command -v npm &>/dev/null || return 0
    npm list -g --depth=0 2>/dev/null | grep -qF "$pkg" \
        && { SKIPPED_PKGS+=("$pkg"); return 0; }
    X npm install -g "$pkg" \
        && { INSTALL_COUNT=$(( INSTALL_COUNT + 1 )); _ok "  $pkg (npm)"; } \
        || { FAILED_PKGS+=("npm:${pkg}"); FAILED_REMEDIATION+=("  npm install -g ${pkg}"); }
}

# ── cargo_batch ───────────────────────────────────────────────────────────────
cargo_batch() {
    local label="$1"; shift
    command -v cargo &>/dev/null || {
        FAILED_PKGS+=("cargo:${label} (cargo unavailable)")
        FAILED_REMEDIATION+=("  Install rustup first: curl https://sh.rustup.rs | sh")
        return 0
    }
    _section "cargo: ${label}"

    if [[ $OPT_FAST -eq 1 ]]; then
        # NOTE: Subshells cannot mutate parent arrays. SKIPPED_PKGS, FAILED_PKGS,
        # and INSTALL_COUNT are NOT updated in parallel mode — counts in the final
        # report will be approximate. All output is still captured to log via tee.
        local -a pids=() ct
        for ct in "$@"; do
            (
                if cargo install --list 2>/dev/null | grep -q "^${ct} "; then
                    printf "${DIM}  ⊘  skip: %s (already installed)${RESET}\n" "$ct"
                    exit 0
                fi
                if X cargo install "$ct"; then
                    printf "${GREEN}  ✔${RESET}  %s (cargo)\n" "$ct"
                else
                    printf "${RED}  ✘${RESET}  %s (cargo) — retry: cargo install %s\n" "$ct" "$ct"
                fi
            ) &
            pids+=($!)
            BG_PIDS+=($!)
            if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
                local finished_pid="${pids[0]}"
                wait "${finished_pid}" 2>/dev/null || true
                pids=("${pids[@]:1}")
                BG_PIDS=("${BG_PIDS[@]/${finished_pid}}")
            fi
        done
        wait "${pids[@]}" 2>/dev/null || true
    else
        for ct in "$@"; do
            cargo install --list 2>/dev/null | grep -q "^${ct} " \
                && { _skip "$ct"; continue; }
            X cargo install "$ct" \
                && { INSTALL_COUNT=$(( INSTALL_COUNT + 1 )); _ok "  ${ct}"; } \
                || { FAILED_PKGS+=("cargo:${ct}"); FAILED_REMEDIATION+=("  cargo install ${ct}"); }
        done
    fi
}

# ══════════════════════════════════════════════════════════════════════════════
#  FAST MODE — enable pacman ParallelDownloads once
# ══════════════════════════════════════════════════════════════════════════════
_enable_fast_pacman() {
    if grep -q "^ParallelDownloads" /etc/pacman.conf; then
        X sudo sed -i "s/^ParallelDownloads.*/ParallelDownloads = ${PACMAN_PARALLEL_DL}/" /etc/pacman.conf
    elif grep -q "^#ParallelDownloads" /etc/pacman.conf; then
        X sudo sed -i "s/^#ParallelDownloads.*/ParallelDownloads = ${PACMAN_PARALLEL_DL}/" /etc/pacman.conf
    else
        X sudo sed -i "/^\[options\]/a ParallelDownloads = ${PACMAN_PARALLEL_DL}" /etc/pacman.conf
    fi
    _ok "pacman ParallelDownloads = ${PACMAN_PARALLEL_DL}"
}

# ══════════════════════════════════════════════════════════════════════════════
#  BENCHMARK — stored in JSON; compared across runs
# ══════════════════════════════════════════════════════════════════════════════
_run_benchmark() {
    [[ $OPT_BENCHMARK -eq 0 ]] && return 0
    _section "Benchmark"

    command -v hyperfine &>/dev/null || { _warn "hyperfine not installed — skipping benchmark"; return 0; }

    local ts
    ts=$(date -Iseconds)
    local results=()

    _info "Benchmarking installed tools…"
    local -A cmds=(
        ["fd_search"]="fd --type f . /usr/lib -x echo > /dev/null"
        ["rg_search"]="rg 'fn main' /usr/lib --type rust -l 2>/dev/null | head -5"
        ["eza_list"]="eza /usr/bin > /dev/null"
        ["bat_render"]="bat /etc/pacman.conf > /dev/null"
        ["starship"]="starship prompt --terminal-width=80"
    )

    for name in "${!cmds[@]}"; do
        local mean
        mean=$(hyperfine --warmup 3 --runs 10 --export-json /tmp/bench-"$name".json \
                "${cmds[$name]}" 2>/dev/null \
               | grep '"mean"' | head -1 | grep -oP '[\d.]+' || echo "N/A")
        results+=("\"${name}\":\"${mean}ms\"")
        _dim "${name}: ${mean}ms"
    done

    # Write JSON report
    {
        printf '{"timestamp":"%s","version":"%s","results":{' "$ts" "$SCRIPT_VERSION"
        printf '%s' "$(IFS=','; echo "${results[*]}")"
        printf '}}\n'
    } >> "${BENCH_FILE}"

    _ok "Benchmark saved → ${BENCH_FILE}"
}

# ══════════════════════════════════════════════════════════════════════════════
#  HELP
# ══════════════════════════════════════════════════════════════════════════════
_usage() {
    cat << EOF

  ${BOLD}${CYAN}arch_dev_setup.sh v${SCRIPT_VERSION}${RESET}

  ${BOLD}MODES${RESET}  (pick one)
    ${CYAN}--minimal${RESET}    Shell · CLI · fonts · Brave                 (~5 min)
    ${CYAN}--dev${RESET}        Minimal + Rust · Python · Node · editors    (~20 min)
    ${CYAN}--ml${RESET}         Dev + AI/ML in isolated conda env           (~40 min)
    ${CYAN}--full${RESET}       Everything                                  (~60 min)

  ${BOLD}FLAGS${RESET}
    ${YELLOW}--yes, -y${RESET}        Unattended mode — auto-answer all prompts with defaults
    ${YELLOW}--gpu-choice N${RESET}   Pre-select GPU driver: 1=nvidia 2=lts 3=open 4=skip
    ${YELLOW}--fast${RESET}           ParallelDownloads + parallel AUR + parallel cargo
    ${YELLOW}--dry-run${RESET}        Print every action; install nothing
    ${YELLOW}--resume${RESET}         Skip phases already completed (reads state file)
    ${YELLOW}--no-reflector${RESET}   Skip mirror refresh (use 24 h cache or current list)
    ${YELLOW}--phase NAME${RESET}     Run only one named phase
    ${YELLOW}--list-phases${RESET}    Print all phase names and exit
    ${YELLOW}--benchmark${RESET}      Run tool benchmarks after install; save to JSON
    ${YELLOW}--clear-state${RESET}    Remove state file (force full reinstall on next run)
    ${YELLOW}--help${RESET}           Show this message

  ${BOLD}EXAMPLES${RESET}
    bash ${SCRIPT_NAME} --full --fast --yes --gpu-choice 3   ${DIM}# fully autonomous${RESET}
    bash ${SCRIPT_NAME} --dev --fast
    bash ${SCRIPT_NAME} --ml --fast --resume
    bash ${SCRIPT_NAME} --phase rust --dry-run
    bash ${SCRIPT_NAME} --dev --fast --no-reflector --resume
    bash ${SCRIPT_NAME} --benchmark --phase cli

  ${DIM}Log: ${LOG_FILE}
  State: ${STATE_FILE}${RESET}

EOF
}

_list_phases() {
    # Use a loop — one printf per row so ANSI escape codes
    # don't corrupt the %-Ns column width calculation.
    local -A desc=(
        [preflight]="Safety checks, internet, disk space, base deps"
        [mirrors]="Reflector mirror update (cached 24 h)"
        [base]="Full system upgrade + paru (AUR helper)"
        [gpu]="NVIDIA driver selection (auto-detected)"
        [fonts]="System fonts + nerd fonts + cursors"
        [cli]="Winner-pick CLI tools (no duplicates)"
        [sysutils]="System utilities + firewall + services"
        [version_managers]="pyenv · uv · fnm · mise · sdkman · rustup"
        [python]="Python ecosystem via pipx (isolated)"
        [rust]="Rust toolchain + cargo extensions + ratatui"
        [node]="fnm · bun · deno · pnpm + global tools"
        [dev_tools]="Compilers · DBs · Docker · K8s · editors"
        [ml_stack]="Miniconda · PyTorch · LLMs · AI CLI (isolated)"
        [shell]="zsh · oh-my-zsh · starship · env bootstrap"
        [neovim]="Neovim + LazyVim starter config"
    )

    # Ordered list (associative arrays don't preserve order in bash)
    local -a order=(
        preflight mirrors base gpu fonts cli sysutils
        version_managers python rust node dev_tools
        ml_stack shell neovim
    )

    printf "\n  ${BOLD}Available phases:${RESET}\n\n"
    for phase in "${order[@]}"; do
        printf "    ${CYAN}%-20s${RESET}  %s\n" "$phase" "${desc[$phase]}"
    done
    printf "\n  ${DIM}Usage: bash %s --phase <name> [--dry-run]${RESET}\n\n" "$SCRIPT_NAME"
}

# ══════════════════════════════════════════════════════════════════════════════
#  ARG PARSER
# ══════════════════════════════════════════════════════════════════════════════
_parse_args() {
    [[ $# -eq 0 ]] && { _usage; exit 0; }
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --minimal)      MODE="minimal"       ;;
            --dev)          MODE="dev"           ;;
            --ml)           MODE="ml"            ;;
            --full)         MODE="full"          ;;
            --fast)         OPT_FAST=1           ;;
            --dry-run)      OPT_DRY=1            ;;
            --resume)       OPT_RESUME=1         ;;
            --no-reflector) OPT_NO_REFLECTOR=1   ;;
            --benchmark)    OPT_BENCHMARK=1      ;;
            --list-phases)  OPT_LIST_PHASES=1    ;;
            --yes|-y)       OPT_YES=1            ;;
            --gpu-choice)
                OPT_GPU_CHOICE="${2:-}"
                [[ -z "$OPT_GPU_CHOICE" ]] && _fatal "--gpu-choice requires a value (1, 2, 3, or 4)"
                [[ ! "$OPT_GPU_CHOICE" =~ ^[1-4]$ ]] && _fatal "--gpu-choice must be 1, 2, 3, or 4"
                shift ;;
            --phase)        OPT_PHASE="${2:-}";  [[ -z "$OPT_PHASE" ]] && _fatal "--phase requires a name"; shift ;;
            --clear-state)  _state_clear;        exit 0 ;;
            --help|-h)      _usage;              exit 0 ;;
            *) _fail "Unknown option: $1"; _usage; exit 1 ;;
        esac
        shift
    done

    [[ $OPT_LIST_PHASES -eq 1 ]] && { _list_phases; exit 0; }
    [[ -z "$MODE" && -z "$OPT_PHASE" ]] && { _fail "No mode selected."; _usage; exit 1; }
}

# ══════════════════════════════════════════════════════════════════════════════
#  CONFIRM PROMPT  (Y as default)
# ══════════════════════════════════════════════════════════════════════════════
_confirm() {
    local prompt="$1" default="${2:-y}"

    # --yes / -y flag: skip all reads and auto-accept the default.
    # Logged so the audit trail shows what was auto-answered.
    if [[ $OPT_YES -eq 1 ]]; then
        _dim "[auto-yes] ${prompt} → ${default}"
        [[ "${default,,}" == "y" ]]
        return $?
    fi

    local options
    if [[ "$default" == "y" ]]; then
        options="${GREEN}Y${RESET}/n"
    else
        options="y/${GREEN}N${RESET}"
    fi
    printf "  %s [%b] " "$prompt" "$options"
    local ans
    # -t 30: auto-accept default after 30s. Prevents permanent hang if user
    # launches the script and walks away before the prompt appears.
    read -r -t 30 ans || ans="$default"
    ans="${ans:-$default}"
    [[ "${ans,,}" == "y" ]]
}

_confirm_start() {
    local mode="$1"

    printf "\n${BOLD}${MAGENTA}  ╔══════════════════════════════════════════════╗${RESET}\n"
    printf "${BOLD}${MAGENTA}  ║  ARCH DEV SETUP  ·  v%-4s                   ║${RESET}\n" "$SCRIPT_VERSION"
    printf "${BOLD}${MAGENTA}  ╚══════════════════════════════════════════════╝${RESET}\n\n"

    printf "  ${BOLD}Mode:${RESET}   ${CYAN}%s${RESET}\n"   "${mode^^}"
    printf "  ${BOLD}Phases:${RESET} ${DIM}%s${RESET}\n"   "${MODE_PHASES[$mode]}"
    printf "  ${BOLD}Fast:${RESET}   %s\n"                  "$( (( OPT_FAST ))   && echo "${YELLOW}yes${RESET}" || echo "${DIM}no${RESET}" )"
    printf "  ${BOLD}Dry run:${RESET}%s\n"                  "$( (( OPT_DRY ))    && echo "${MAGENTA}yes (nothing installed)${RESET}" || echo "${DIM}no${RESET}" )"
    printf "  ${BOLD}Resume:${RESET} %s\n"                  "$( (( OPT_RESUME )) && echo "${GREEN}yes${RESET}" || echo "${DIM}no${RESET}" )"
    printf "  ${BOLD}Log:${RESET}    ${DIM}%s${RESET}\n\n"  "$LOG_FILE"

    (( OPT_DRY )) && printf "  ${BOLD}${MAGENTA}DRY RUN — no packages will be installed${RESET}\n\n"

    _confirm "Proceed with installation?" "y" || { printf "  Aborted.\n\n"; exit 0; }
    echo ""
}

# ══════════════════════════════════════════════════════════════════════════════
#  MODE → PHASE MAP
#  Order matters: each phase may depend on the previous.
# ══════════════════════════════════════════════════════════════════════════════
declare -A MODE_PHASES
MODE_PHASES[minimal]="base fonts cli sysutils shell neovim"
MODE_PHASES[dev]="base gpu fonts cli sysutils version_managers python rust node dev_tools shell neovim"
MODE_PHASES[ml]="base gpu fonts cli sysutils version_managers python rust node dev_tools ml_stack shell neovim"
MODE_PHASES[full]="base gpu fonts cli sysutils version_managers python rust node dev_tools ml_stack shell neovim"

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE: PREFLIGHT
#  Fast exit on critical failures; warn on soft failures.
# ══════════════════════════════════════════════════════════════════════════════
phase_preflight() {
    local t=$SECONDS
    _section "Preflight checks"

    # Critical
    [[ $EUID -eq 0 ]]   && _fatal "Do not run as root. Use a normal user — sudo is called internally."
    grep -qi "arch" /etc/os-release 2>/dev/null || _fatal "Not Arch Linux."
    ping -c1 -W3 archlinux.org &>/dev/null      || _fatal "No internet connection."

    # Soft
    local free_gb
    free_gb=$(( $(df "${HOME}" --output=avail | tail -1) / 1024 / 1024 ))
    (( free_gb < 10 )) && _warn "Only ${free_gb}GB free on ${HOME} — full install needs ~20GB"

    # Ensure minimal base tools exist before anything else
    local -a need=( git curl wget sudo )
    local -a absent=()
    for d in "${need[@]}"; do command -v "$d" &>/dev/null || absent+=("$d"); done
    [[ ${#absent[@]} -gt 0 ]] && X sudo pacman -S --noconfirm --needed "${absent[@]}"

    _ok "Preflight passed  (${free_gb}GB free, PID $$)"
    PHASE_TIMES[preflight]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE: MIRRORS
#  Cached 24 h; --no-reflector skips entirely; respects --fast.
# ══════════════════════════════════════════════════════════════════════════════
phase_mirrors() {
    local t=$SECONDS

    if [[ $OPT_NO_REFLECTOR -eq 1 ]]; then
        _skip "mirrors (--no-reflector)"
        PHASE_TIMES[mirrors]=$(( SECONDS - t ))
        return 0
    fi

    # Use cache if < TTL
    if [[ -f "${MIRROR_CACHE}" ]]; then
        local age=$(( $(date +%s) - $(stat -c %Y "${MIRROR_CACHE}") ))
        if (( age < MIRROR_CACHE_TTL )); then
            _skip "mirrors (cache is $(( age / 3600 ))h old — TTL 24h)"
            X sudo cp "${MIRROR_CACHE}" /etc/pacman.d/mirrorlist
            PHASE_TIMES[mirrors]=$(( SECONDS - t ))
            return 0
        fi
    fi

    _section "Updating mirrors"
    command -v reflector &>/dev/null || X sudo pacman -S --noconfirm --needed reflector

    _spin_start "Ranking mirrors (this takes ~30s)…"
    X sudo reflector \
        --country DE,FR,NL,GB,US \
        --age 12 --protocol https \
        --sort rate --latest 20 \
        --save /etc/pacman.d/mirrorlist
    _spin_stop

    X sudo cp /etc/pacman.d/mirrorlist "${MIRROR_CACHE}"
    _ok "Mirrors updated and cached → ${MIRROR_CACHE}"
    PHASE_TIMES[mirrors]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 1: BASE — system upgrade + paru
# ══════════════════════════════════════════════════════════════════════════════
phase_base() {
    local t=$SECONDS
    _phase_skip_if_done "base" && return 0
    _phase_banner "Base System + AUR Helper"

    # Enable multilib (idempotent)
    if ! grep -q "^\[multilib\]" /etc/pacman.conf; then
        X sudo sed -i '/^#\[multilib\]/s/^#//; /^\[multilib\]/{n;s/^#//}' /etc/pacman.conf
        _ok "multilib repo enabled"
    fi

    _heartbeat_start "running pacman -Syu"
    _spin_start "Full system upgrade…"
    X sudo pacman -Syu --noconfirm
    _spin_stop
    _heartbeat_stop
    _ok "System up to date"

    pacman_batch "build-essentials" base-devel git curl wget rsync

    # ── paru ──────────────────────────────────────────────────────────────────
    _section "paru (AUR helper)"
    if ! command -v paru &>/dev/null; then
        local tmp="/tmp/paru-build-${SCRIPT_PID}"
        if X git clone --depth=1 https://aur.archlinux.org/paru.git "$tmp"; then
            ( cd "$tmp" && X makepkg -si --noconfirm ) \
                && { INSTALL_COUNT=$(( INSTALL_COUNT + 1 )); _ok "paru installed"; } \
                || { _fail "paru build failed"; FAILED_PKGS+=("paru"); FAILED_REMEDIATION+=("  git clone https://aur.archlinux.org/paru.git /tmp/paru && cd /tmp/paru && makepkg -si"); }
            rm -rf "$tmp"
        else
            _fail "Failed to clone paru"; FAILED_PKGS+=("paru (clone failed)")
        fi
    else
        _skip "paru ($(paru --version 2>/dev/null | head -1))"
    fi

    _state_done "base"
    PHASE_TIMES[base]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 2: GPU DRIVERS
# ══════════════════════════════════════════════════════════════════════════════
phase_gpu() {
    local t=$SECONDS
    _phase_skip_if_done "gpu" && return 0
    _phase_banner "GPU Drivers"

    local -a common=(
        nvidia-utils nvidia-settings lib32-nvidia-utils
        opencl-nvidia libvdpau vulkan-icd-loader lib32-vulkan-icd-loader
    )

    # Resolve NVIDIA driver choice. Priority:
    #   1. --gpu-choice N flag (set at launch — fully non-interactive)
    #   2. NVIDIA_CHOICE env var (allows: NVIDIA_CHOICE=3 bash arch_dev_setup.sh --full)
    #   3. --yes flag: auto-select choice 1 (proprietary, safest default)
    #   4. Interactive prompt with 30s timeout → defaults to 1 if no answer
    if lspci 2>/dev/null | grep -qi nvidia && [[ -z "$NVIDIA_CHOICE" ]]; then
        if [[ -n "$OPT_GPU_CHOICE" ]]; then
            NVIDIA_CHOICE="$OPT_GPU_CHOICE"
            _info "GPU driver pre-selected via --gpu-choice: ${NVIDIA_CHOICE}"
        elif [[ $OPT_YES -eq 1 ]]; then
            NVIDIA_CHOICE="1"
            _info "GPU driver auto-selected (--yes): nvidia proprietary (choice 1)"
        else
            printf "\n  ${YELLOW}NVIDIA GPU detected.${RESET} Choose driver:\n\n"
            printf "    ${CYAN}1${RESET}) nvidia        — GTX 900+ / RTX series (proprietary)\n"
            printf "    ${CYAN}2${RESET}) nvidia-lts    — LTS kernel build\n"
            printf "    ${CYAN}3${RESET}) nvidia-open   — open kernel modules (RTX 20+)\n"
            printf "    ${CYAN}4${RESET}) Skip          — not using NVIDIA\n\n"
            printf "  Choice [1-4, default=1, auto-selects in 30s]: "
            # -t 30: prevents permanent hang if user walks away mid-run
            read -r -t 30 NVIDIA_CHOICE || true
            NVIDIA_CHOICE="${NVIDIA_CHOICE:-1}"
        fi
    fi

    case "${NVIDIA_CHOICE:-4}" in
        1) pacman_batch "nvidia"       nvidia      "${common[@]}"; paru_one "nvidia-dkms" ;;
        2) pacman_batch "nvidia-lts"   nvidia-lts  "${common[@]}" ;;
        3) pacman_batch "nvidia-open"  nvidia-open "${common[@]}" ;;
        4) _skip "GPU drivers (none selected)" ;;
        *) _warn "Invalid choice '${NVIDIA_CHOICE}' — skipping GPU drivers" ;;
    esac

    _state_done "gpu"
    PHASE_TIMES[gpu]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 3: FONTS
# ══════════════════════════════════════════════════════════════════════════════
phase_fonts() {
    local t=$SECONDS
    _phase_skip_if_done "fonts" && return 0
    _phase_banner "Fonts"

    pacman_batch "fonts-pacman" \
        ttf-liberation ttf-dejavu \
        noto-fonts noto-fonts-emoji noto-fonts-cjk \
        ttf-font-awesome ttf-jetbrains-mono ttf-fira-code \
        ttf-cascadia-code adobe-source-code-pro-fonts terminus-font

    paru_batch "fonts-aur" \
        ttf-nerd-fonts-symbols nerd-fonts-jetbrains-mono \
        ttf-ms-fonts bibata-cursor-theme

    X fc-cache -fv &>/dev/null && _ok "Font cache refreshed"

    _state_done "fonts"
    PHASE_TIMES[fonts]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 4: CLI TOOLS  (winner picks — see header for rationale)
# ══════════════════════════════════════════════════════════════════════════════
phase_cli() {
    local t=$SECONDS
    _phase_skip_if_done "cli" && return 0
    _phase_banner "Essential CLI Tools (no duplicates)"

    pacman_batch "cli-core" \
        vim neovim nano \
        btop zellij \
        fzf ripgrep fd bat eza \
        unzip zip p7zip tar \
        jq bc tree \
        man-db man-pages bash-completion zsh \
        lsof net-tools nmap strace less which \
        mediainfo ffmpegthumbnailer highlight atool \
        glow silicon tokei hyperfine \
        procs dust duf delta \
        ranger python-pillow w3m

    paru_batch "cli-aur" \
        bottom atuin \
        zoxide starship tealdeer navi \
        yazi broot difftastic \
        xh hurl oha miniserve \
        pueue just watchexec \
        grex sd choose mdcat

    _state_done "cli"
    PHASE_TIMES[cli]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 5: SYSTEM UTILITIES
# ══════════════════════════════════════════════════════════════════════════════
phase_sysutils() {
    local t=$SECONDS
    _phase_skip_if_done "sysutils" && return 0
    _phase_banner "System Utilities"

    pacman_batch "sysutils" \
        polkit sudo reflector pacman-contrib \
        cronie ufw \
        udisks2 gvfs gvfs-mtp \
        ntfs-3g dosfstools exfatprogs btrfs-progs e2fsprogs \
        lsblk parted smartmontools \
        usbutils pciutils dmidecode \
        acpi lm_sensors sysstat iotop iftop \
        openssh gnupg age \
        powertop thermald

    paru_batch "sysutils-aur" timeshift gparted

    _section "Enabling services"
    local -A svcs=(
        [cronie]="task scheduler"
        [ufw]="firewall"
        [thermald]="thermal management"
    )
    for svc in "${!svcs[@]}"; do
        if X sudo systemctl enable --now "$svc" 2>/dev/null; then
            _ok "${svc} (${svcs[$svc]})"
        else
            _warn "${svc} failed to start — may not be supported on this hardware"
        fi
    done

    X sudo ufw default deny incoming &>/dev/null
    X sudo ufw allow ssh &>/dev/null
    _ok "ufw: deny incoming, allow SSH"

    _state_done "sysutils"
    PHASE_TIMES[sysutils]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 6: VERSION MANAGERS
#  One winner per job — no redundant tools:
#    Python   pyenv + uv  (uv replaces pip/venv/virtualenv, 100× faster)
#    Node     fnm         (Rust, replaces nvm/volta/n — 40× faster)
#    General  mise        (Rust, replaces asdf — 10× faster)
#    JVM      sdkman      (de facto standard for Java ecosystem)
#    Rust     rustup      (official — no alternative needed)
# ══════════════════════════════════════════════════════════════════════════════
phase_version_managers() {
    local t=$SECONDS
    _phase_skip_if_done "version_managers" && return 0
    _phase_banner "Version Managers"

    # ── rustup ────────────────────────────────────────────────────────────────
    _section "rustup (official Rust toolchain manager)"
    if ! command -v rustup &>/dev/null; then
        _spin_start "Installing rustup…"
        X sh -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
            | sh -s -- -y --no-modify-path --default-toolchain stable --profile default'
        _spin_stop
        # shellcheck source=/dev/null
        source "${HOME}/.cargo/env" 2>/dev/null || true
        _ok "rustup + stable toolchain"
    else
        _skip "rustup ($(rustup --version 2>/dev/null | head -1))"
        X rustup update 2>/dev/null || true
    fi
    # Additional targets/components always idempotent
    X rustup toolchain install nightly --no-self-update 2>/dev/null || true
    X rustup component add rust-analyzer clippy rustfmt 2>/dev/null || true
    X rustup target add wasm32-unknown-unknown 2>/dev/null || true
    _ok "rust targets: stable · nightly · wasm32"

    # ── pyenv ─────────────────────────────────────────────────────────────────
    _section "pyenv (Python version manager)"
    pacman_batch "pyenv" pyenv

    # ── uv (replaces pip + venv + virtualenv + pip-tools) ────────────────────
    _section "uv — ultrafast pip/venv replacement (Rust, 100× faster)"
    if ! command -v uv &>/dev/null; then
        X sh -c 'curl -LsSf https://astral.sh/uv/install.sh | sh'
        _ok "uv installed"
    else
        _skip "uv ($(uv --version 2>/dev/null))"
    fi

    # ── fnm (replaces nvm · volta · n) ───────────────────────────────────────
    _section "fnm — Node version manager (Rust, 40× faster than nvm)"
    if ! command -v fnm &>/dev/null; then
        X sh -c 'curl -fsSL https://fnm.vercel.app/install | bash --install-dir "${HOME}/.fnm" --skip-shell'
        _ok "fnm installed"
    else
        _skip "fnm ($(fnm --version 2>/dev/null))"
    fi

    # ── mise (replaces asdf) ─────────────────────────────────────────────────
    _section "mise — universal version manager (replaces asdf)"
    if ! command -v mise &>/dev/null; then
        X sh -c 'curl https://mise.jdx.dev/install.sh | sh'
        _ok "mise installed"
    else
        _skip "mise ($(mise --version 2>/dev/null))"
    fi

    # ── sdkman ───────────────────────────────────────────────────────────────
    _section "sdkman — JVM toolchain manager (Java · Kotlin · Gradle · Maven)"
    if [[ ! -d "${HOME}/.sdkman" ]]; then
        X sh -c 'curl -s "https://get.sdkman.io" | bash'
        _ok "sdkman installed"
    else
        _skip "sdkman"
    fi

    _state_done "version_managers"
    PHASE_TIMES[version_managers]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 7: PYTHON ECOSYSTEM
#  All tools via pipx — fully isolated, never pollutes system Python.
# ══════════════════════════════════════════════════════════════════════════════
phase_python() {
    local t=$SECONDS
    _phase_skip_if_done "python" && return 0
    _phase_banner "Python Ecosystem (pipx-isolated)"

    pacman_batch "python-core" python python-pip python-pipx python-virtualenv

    _section "Project / build managers (pipx)"
    for tool in poetry pdm hatch; do pipx_one "$tool"; done

    _section "Linting / formatting (pipx)"
    for tool in black ruff mypy pylint flake8 isort autopep8 bandit; do
        pipx_one "$tool"
    done

    _section "Dev productivity tools (pipx)"
    for tool in pre-commit cookiecutter httpie yt-dlp rich-cli posting pgcli litecli mycli; do
        pipx_one "$tool"
    done

    _state_done "python"
    PHASE_TIMES[python]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 8: RUST ECOSYSTEM
# ══════════════════════════════════════════════════════════════════════════════
phase_rust() {
    local t=$SECONDS
    _phase_skip_if_done "rust" && return 0
    _phase_banner "Rust Ecosystem + ratatui"

    # shellcheck source=/dev/null
    [[ -f "${HOME}/.cargo/env" ]] && source "${HOME}/.cargo/env"
    command -v cargo &>/dev/null \
        || { _fail "cargo unavailable — run version_managers first"; return 1; }

    cargo_batch "cargo-extensions" \
        cargo-edit cargo-watch cargo-expand \
        cargo-audit cargo-deny cargo-outdated \
        cargo-nextest cargo-tarpaulin \
        cargo-generate cargo-make cargo-release \
        cargo-update cargo-bloat bacon

    _section "Rust CLI tools (pacman — no compilation needed)"
    pacman_batch "rust-cli-pacman" \
        alacritty helix gitui lazygit \
        ripgrep fd bat eza delta \
        hyperfine procs dust duf \
        zoxide starship silicon glow tokei

    paru_batch "rust-cli-aur" \
        wezterm zed-bin lapce \
        yazi broot atuin \
        difftastic xh oha miniserve

    # ── ratatui starter project ───────────────────────────────────────────────
    _section "ratatui starter project"
    local demo_dir="${HOME}/projects/ratatui-demo"
    if [[ ! -d "$demo_dir" ]]; then
        mkdir -p "${HOME}/projects"
        X cargo new "$demo_dir" 2>/dev/null || true
        if [[ -f "${demo_dir}/Cargo.toml" ]]; then
            cat >> "${demo_dir}/Cargo.toml" << 'TOML'

[dependencies]
ratatui     = "0.26"
crossterm   = "0.27"
color-eyre  = "0.6"
tokio       = { version = "1", features = ["full"] }
TOML
            cat > "${demo_dir}/src/main.rs" << 'RUST'
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    loop {
        terminal.draw(|f| {
            let area = f.area();
            f.render_widget(
                Paragraph::new("Hello ratatui! Press 'q' to quit.")
                    .block(Block::default().borders(Borders::ALL).title("Demo")),
                area,
            );
        })?;
        if let Event::Key(k) = event::read()? {
            if k.code == KeyCode::Char('q') { break; }
        }
    }
    ratatui::restore();
    Ok(())
}
RUST
            _ok "ratatui demo → ${demo_dir}  (cargo run to start)"
        fi
    else
        _skip "ratatui demo (already exists)"
    fi

    _state_done "rust"
    PHASE_TIMES[rust]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 9: NODE
#  fnm for version management (single winner), bun + deno as runtimes,
#  pnpm as default package manager (3× faster than npm, disk-efficient).
# ══════════════════════════════════════════════════════════════════════════════
phase_node() {
    local t=$SECONDS
    _phase_skip_if_done "node" && return 0
    _phase_banner "Node — fnm · bun · deno · pnpm"

    pacman_batch "node-base" nodejs npm

    _section "pnpm (disk-efficient, fast package manager)"
    if command -v pnpm &>/dev/null; then
        _skip "pnpm ($(pnpm --version 2>/dev/null))"
    elif X sh -c 'curl -fsSL https://get.pnpm.io/install.sh | sh -'; then
        INSTALL_COUNT=$(( INSTALL_COUNT + 1 ))
        _ok "pnpm installed"
    else
        _fail "pnpm install failed"
        FAILED_PKGS+=("pnpm")
        FAILED_REMEDIATION+=("  curl -fsSL https://get.pnpm.io/install.sh | sh -")
    fi

    _section "bun (fast JS runtime + bundler)"
    if command -v bun &>/dev/null; then
        _skip "bun ($(bun --version 2>/dev/null))"
    elif X sh -c 'curl -fsSL https://bun.sh/install | bash'; then
        INSTALL_COUNT=$(( INSTALL_COUNT + 1 ))
        _ok "bun installed"
    else
        _fail "bun install failed"
        FAILED_PKGS+=("bun")
        FAILED_REMEDIATION+=("  curl -fsSL https://bun.sh/install | bash")
    fi

    _section "deno (secure, modern JS/TS runtime)"
    if command -v deno &>/dev/null; then
        _skip "deno ($(deno --version 2>/dev/null | head -1))"
    elif X sh -c 'curl -fsSL https://deno.land/install.sh | sh'; then
        INSTALL_COUNT=$(( INSTALL_COUNT + 1 ))
        _ok "deno installed"
    else
        _fail "deno install failed"
        FAILED_PKGS+=("deno")
        FAILED_REMEDIATION+=("  curl -fsSL https://deno.land/install.sh | sh")
    fi

    _section "Global npm tools"
    for tool in \
        typescript ts-node tsx nodemon pm2 \
        prettier eslint vite esbuild turbo \
        vercel netlify-cli wrangler; do
        npm_global "$tool"
    done

    _state_done "node"
    PHASE_TIMES[node]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 10: DEV TOOLS
# ══════════════════════════════════════════════════════════════════════════════
phase_dev_tools() {
    local t=$SECONDS
    _phase_skip_if_done "dev_tools" && return 0
    _phase_banner "Dev Tools — Compilers · DBs · DevOps · Editors"

    _section "Compilers + build tools"
    pacman_batch "compilers" \
        gcc clang cmake make meson ninja pkgconf \
        autoconf automake libtool gdb valgrind strace ltrace git-lfs

    _section "Go"
    pacman_batch "go" go

    _section "JVM base (sdkman handles version switching)"
    pacman_batch "jvm" jdk-openjdk kotlin

    _section "Database servers"
    pacman_batch "db-servers" postgresql mariadb sqlite redis
    paru_one "mongodb-bin"

    _section "Database GUI (dbeaver — others via jetbrains-toolbox)"
    paru_one "dbeaver"

    _section "Database CLI"
    for cli in pgcli mycli litecli; do pipx_one "$cli"; done
    paru_one "mongosh-bin"

    _section "Docker"
    pacman_batch "docker" docker docker-compose
    paru_one "lazydocker"
    X sudo systemctl enable --now docker && _ok "docker service enabled"
    X sudo usermod -aG docker "$USER"   && _ok "user added to docker group (reboot to apply)"

    _section "Kubernetes"
    pacman_batch "k8s" kubectl helm
    paru_batch "k8s-aur" k9s kind minikube

    _section "Infrastructure as Code"
    paru_batch "iac" terraform-bin ansible-core pulumi-bin

    _section "CI/CD"
    paru_one "act"   # run GitHub Actions locally

    _section "Cloud CLIs"
    paru_batch "cloud-cli" aws-cli-v2-bin google-cloud-cli

    _section "API testing tools"
    paru_batch "api-tools" postman-bin insomnia-bin bruno-bin

    _section "Editors (neovim configured in neovim phase)"
    paru_batch "editors" visual-studio-code-bin vscodium-bin jetbrains-toolbox zed-bin lapce

    _section "Git tools"
    pacman_batch "git-tools" git git-lfs tig
    paru_batch "git-aur" lazygit gitui gh glab

    _section "Browser"
    paru_one "brave-bin"

    _section "Zathura (PDF/DJVU/PS viewer)"
    pacman_batch "zathura" \
        zathura zathura-pdf-mupdf zathura-ps zathura-djvu zathura-cb

    _section "Security tools"
    pacman_batch "security" gnupg openssh age
    paru_batch "security-aur" sops trivy-bin hadolint-bin

    _state_done "dev_tools"
    PHASE_TIMES[dev_tools]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 11: ML STACK
#  Deliberately isolated — only runs on --ml / --full.
#  CUDA + PyTorch are multi-GB and slow. They live in conda env 'ml-base'
#  and NEVER touch the system Python. Run standalone:
#      bash arch_dev_setup.sh --phase ml_stack
# ══════════════════════════════════════════════════════════════════════════════
phase_ml_stack() {
    local t=$SECONDS
    _phase_skip_if_done "ml_stack" && return 0
    _phase_banner "AI / ML Stack (fully isolated)"

    # ── Miniconda ─────────────────────────────────────────────────────────────
    _section "Miniconda3 (isolated Python env manager)"
    if ! command -v conda &>/dev/null && [[ ! -d "${HOME}/miniconda3" ]]; then
        _spin_start "Downloading Miniconda3…"
        X wget -qO /tmp/mc-install.sh \
            "https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh"
        _spin_stop
        X bash /tmp/mc-install.sh -b -p "${HOME}/miniconda3"
        X "${HOME}/miniconda3/bin/conda" init zsh bash
        X "${HOME}/miniconda3/bin/conda" install -n base -c conda-forge mamba -y
        X "${HOME}/miniconda3/bin/conda" config --set auto_activate_base false
        rm -f /tmp/mc-install.sh
        _ok "miniconda3 + mamba installed (base auto-activate disabled)"
    else
        _skip "conda"
    fi

    local conda_exe="${HOME}/miniconda3/bin/conda"
    [[ -x "$conda_exe" ]] || conda_exe="$(command -v conda 2>/dev/null || echo "")"
    [[ -z "$conda_exe" ]] && {
        _warn "conda unavailable — rerun this phase after shell restart"
        FAILED_PKGS+=("conda:ml-base")
        FAILED_REMEDIATION+=("  source ~/.zshrc && bash ${SCRIPT_NAME} --phase ml_stack")
        return 0
    }

    # ── Ollama ────────────────────────────────────────────────────────────────
    _section "Ollama (local LLM runner)"
    if ! command -v ollama &>/dev/null; then
        X sh -c 'curl -fsSL https://ollama.ai/install.sh | sh'
        X sudo systemctl enable --now ollama 2>/dev/null || true
        _ok "ollama installed + service enabled"
    else
        _skip "ollama"
    fi

    # ── AI CLI tools ──────────────────────────────────────────────────────────
    _section "AI CLI tools"
    pipx_one "aider-chat"
    pipx_one "shell-gpt"
    pipx_one "llm"
    npm_global "@anthropic-ai/claude-code"
    paru_batch "ai-desktop-apps" aichat lm-studio-bin jan-bin

    # ── ml-base conda environment ─────────────────────────────────────────────
    _section "Creating ml-base conda environment (Python 3.11)"
    X "$conda_exe" create -n ml-base python=3.11 -y 2>/dev/null || true

    _info "Installing PyTorch (CUDA 12.1, falls back to CPU)…"
    _heartbeat_start "installing PyTorch (large download — can take 10+ min)"
    _spin_start "Installing PyTorch…"
    X "$conda_exe" run -n ml-base conda install -y \
        pytorch torchvision torchaudio pytorch-cuda=12.1 \
        -c pytorch -c nvidia 2>/dev/null \
    || X "$conda_exe" run -n ml-base conda install -y \
        pytorch torchvision torchaudio cpuonly \
        -c pytorch 2>/dev/null \
    || _warn "PyTorch failed — activate ml-base and retry manually"
    _spin_stop
    _heartbeat_stop

    _info "Installing ML + LLM Python libraries…"
    _heartbeat_start "pip installing ML libraries"
    _spin_start "Installing ML libraries (this takes a while)…"
    X "$conda_exe" run -n ml-base pip install --quiet \
        numpy pandas scipy scikit-learn \
        matplotlib seaborn plotly \
        transformers diffusers accelerate peft bitsandbytes einops timm \
        lightning fastai \
        langchain langchain-community langgraph \
        llama-index llama-cpp-python chromadb \
        jupyterlab notebook ipython voila \
        streamlit gradio \
        mlflow wandb optuna \
        anthropic openai google-generativeai \
        2>/dev/null \
    && _ok "ML libs installed in conda env 'ml-base'" \
    || _warn "Some ML packages failed — activate ml-base and pip install manually"
    _spin_stop
    _heartbeat_stop

    _state_done "ml_stack"
    PHASE_TIMES[ml_stack]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 12: SHELL
#  Writes a single bootstrap file ~/.zshrc_devenv; sources it from ~/.zshrc.
#  The file is always overwritten (idempotent) so updates propagate.
# ══════════════════════════════════════════════════════════════════════════════
phase_shell() {
    local t=$SECONDS
    _phase_skip_if_done "shell" && return 0
    _phase_banner "Shell — zsh · oh-my-zsh · starship · env bootstrap"

    # ── oh-my-zsh ─────────────────────────────────────────────────────────────
    if [[ ! -d "${HOME}/.oh-my-zsh" ]]; then
        X sh -c 'RUNZSH=no CHSH=no curl -fsSL \
            https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh | bash'
        _ok "oh-my-zsh installed"
    else
        _skip "oh-my-zsh"
    fi

    local ZSH_CUSTOM="${ZSH_CUSTOM:-${HOME}/.oh-my-zsh/custom}"

    # ── zsh plugins ───────────────────────────────────────────────────────────
    local -A plugins=(
        ["zsh-autosuggestions"]="zsh-users/zsh-autosuggestions"
        ["zsh-syntax-highlighting"]="zsh-users/zsh-syntax-highlighting"
        ["zsh-completions"]="zsh-users/zsh-completions"
    )
    for name in "${!plugins[@]}"; do
        if [[ ! -d "${ZSH_CUSTOM}/plugins/${name}" ]]; then
            X git clone --depth=1 \
                "https://github.com/${plugins[$name]}" \
                "${ZSH_CUSTOM}/plugins/${name}" \
            && _ok "plugin: ${name}"
        else
            _skip "plugin: ${name}"
        fi
    done

    # ── powerlevel10k ─────────────────────────────────────────────────────────
    if [[ ! -d "${ZSH_CUSTOM}/themes/powerlevel10k" ]]; then
        X git clone --depth=1 \
            https://github.com/romkatv/powerlevel10k.git \
            "${ZSH_CUSTOM}/themes/powerlevel10k" \
        && _ok "theme: powerlevel10k"
    else
        _skip "theme: powerlevel10k"
    fi

    # ── env bootstrap file (always refreshed) ────────────────────────────────
    cat > "${HOME}/.zshrc_devenv" << 'DEVENV'
# ════════════════════════════════════════════════════════════════════════
#  arch-dev-setup: environment bootstrap
#  Auto-generated — safe to edit. Re-run setup to regenerate defaults.
# ════════════════════════════════════════════════════════════════════════

# ── Cargo / Rust ─────────────────────────────────────────────────────────────
[[ -f "${HOME}/.cargo/env" ]] && source "${HOME}/.cargo/env"

# ── pyenv ─────────────────────────────────────────────────────────────────────
export PYENV_ROOT="${HOME}/.pyenv"
export PATH="${PYENV_ROOT}/bin:${PATH}"
command -v pyenv &>/dev/null && eval "$(pyenv init -)"

# ── uv (fast pip/venv — replaces pip) ────────────────────────────────────────
export PATH="${HOME}/.local/bin:${PATH}"

# ── fnm (Node version manager — replaces nvm) ────────────────────────────────
export PATH="${HOME}/.fnm:${PATH}"
command -v fnm &>/dev/null && eval "$(fnm env --use-on-cd --shell zsh)"

# ── mise (universal version manager — replaces asdf) ─────────────────────────
command -v mise &>/dev/null && eval "$(mise activate zsh)"

# ── pnpm ──────────────────────────────────────────────────────────────────────
export PNPM_HOME="${HOME}/.local/share/pnpm"
export PATH="${PNPM_HOME}:${PATH}"

# ── bun ───────────────────────────────────────────────────────────────────────
export BUN_INSTALL="${HOME}/.bun"
export PATH="${BUN_INSTALL}/bin:${PATH}"
[[ -s "${HOME}/.bun/_bun" ]] && source "${HOME}/.bun/_bun"

# ── deno ──────────────────────────────────────────────────────────────────────
export DENO_INSTALL="${HOME}/.deno"
export PATH="${DENO_INSTALL}/bin:${PATH}"

# ── go ────────────────────────────────────────────────────────────────────────
export GOPATH="${HOME}/go"
export PATH="${GOPATH}/bin:${PATH}"

# ── sdkman (JVM) ──────────────────────────────────────────────────────────────
export SDKMAN_DIR="${HOME}/.sdkman"
[[ -s "${SDKMAN_DIR}/bin/sdkman-init.sh" ]] && source "${SDKMAN_DIR}/bin/sdkman-init.sh"

# ── conda (lazy-loaded — won't slow down shell startup) ──────────────────────
# Activate with: conda activate ml-base
if [[ -f "${HOME}/miniconda3/etc/profile.d/conda.sh" ]]; then
    source "${HOME}/miniconda3/etc/profile.d/conda.sh"
    conda config --set auto_activate_base false 2>/dev/null || true
fi

# ── atuin (encrypted shell history) ──────────────────────────────────────────
command -v atuin &>/dev/null && eval "$(atuin init zsh --disable-up-arrow)"

# ── zoxide (smart cd) ────────────────────────────────────────────────────────
command -v zoxide &>/dev/null && eval "$(zoxide init zsh)"

# ── starship (prompt) ────────────────────────────────────────────────────────
command -v starship &>/dev/null && eval "$(starship init zsh)"

# ════════════════════════════════════════════════════════════════════════
#  ALIASES  (one tool per job — see script header for rationale)
# ════════════════════════════════════════════════════════════════════════

# System
alias ls='eza --icons --group-directories-first'
alias ll='eza -la --icons --git --group-directories-first'
alias la='eza -a --icons'
alias lt='eza --tree --icons -L 3'
alias cat='bat --style=auto'
alias top='btop'
alias df='duf'
alias du='dust'
alias grep='rg'
alias find='fd'
alias ps='procs'
alias diff='delta'

# Editors
alias vim='nvim'
alias vi='nvim'
alias v='nvim'

# Python
alias py='python'
alias pip='uv pip'
alias venv='uv venv'
alias ipy='ipython'

# Git
alias g='git'
alias gs='git status'
alias gc='git commit'
alias gp='git push'
alias gl='git log --oneline --graph'
alias lg='lazygit'

# Docker
alias dk='docker'
alias dkc='docker compose'
alias dki='docker images'
alias dkps='docker ps'

# Kubernetes
alias k='kubectl'
alias kgp='kubectl get pods'
alias kgs='kubectl get svc'
alias kgd='kubectl get deployments'

# Terraform
alias tf='terraform'
alias tfi='terraform init'
alias tfp='terraform plan'
alias tfa='terraform apply'

# Misc shortcuts
alias please='sudo'
alias sudo='sudo '        # allows aliases to work with sudo
alias reload='source ~/.zshrc && echo "Shell reloaded"'
alias update='sudo pacman -Syu --noconfirm && paru -Syu --noconfirm && pipx upgrade-all'
alias cleanup='sudo paccache -rk2 && paru -Scc --noconfirm && docker system prune -f 2>/dev/null'
alias ports='ss -tulnp'
alias myip='curl -s ifconfig.me && echo'
alias weather='curl -s wttr.in'
alias speedtest='curl -s https://raw.githubusercontent.com/sivel/speedtest-cli/master/speedtest.py | python -'
alias path='echo -e "${PATH//:/\\n}"'
alias clr='clear'

# Dev shortcuts
alias serve='python -m http.server'
alias json='python -m json.tool'
alias uuid="python -c 'import uuid; print(uuid.uuid4())'"
alias ts="date +%s"

# ════════════════════════════════════════════════════════════════════════
#  FUNCTIONS
# ════════════════════════════════════════════════════════════════════════

# Create dir and cd into it
mkcd() { mkdir -p "$1" && cd "$1"; }

# Extract any archive
extract() {
    case "$1" in
        *.tar.bz2)  tar xjf "$1"   ;;
        *.tar.gz)   tar xzf "$1"   ;;
        *.tar.xz)   tar xJf "$1"   ;;
        *.tar.zst)  tar --zstd -xf "$1" ;;
        *.bz2)      bunzip2 "$1"   ;;
        *.gz)       gunzip "$1"    ;;
        *.tar)      tar xf "$1"    ;;
        *.zip)      unzip "$1"     ;;
        *.7z)       7z x "$1"      ;;
        *.rar)      unrar x "$1"   ;;
        *) echo "Unknown archive: $1" ;;
    esac
}

# Quick Python venv in current dir
pyvenv() {
    uv venv "${1:-.venv}" && source "${1:-.venv}/bin/activate"
}

# Git clone and cd
gcl() { git clone "$1" && cd "$(basename "$1" .git)"; }

# Show top 10 biggest files
biggest() { dust -n "${1:-10}" "${2:-.}"; }

# Run a command and benchmark it
bench() { hyperfine --warmup 3 "$@"; }

# ════════════════════════════════════════════════════════════════════════
#  END arch-dev-setup bootstrap
# ════════════════════════════════════════════════════════════════════════
DEVENV

    _ok "~/.zshrc_devenv written"

    # Source from .zshrc (idempotent)
    if ! grep -q "zshrc_devenv" "${HOME}/.zshrc" 2>/dev/null; then
        printf '\n# arch-dev-setup\n[[ -f "${HOME}/.zshrc_devenv" ]] && source "${HOME}/.zshrc_devenv"\n' \
            >> "${HOME}/.zshrc"
        _ok "Sourced in ~/.zshrc"
    else
        _skip "already sourced in ~/.zshrc"
    fi

    # Change default shell
    local zsh_path
    zsh_path="$(command -v zsh)"
    if [[ "$SHELL" != "$zsh_path" ]]; then
        X chsh -s "$zsh_path" "$USER" && _ok "Default shell → zsh (takes effect on next login)"
    else
        _skip "zsh already default shell"
    fi

    _state_done "shell"
    PHASE_TIMES[shell]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE 13: NEOVIM + LAZYVIM
# ══════════════════════════════════════════════════════════════════════════════
phase_neovim() {
    local t=$SECONDS
    _phase_skip_if_done "neovim" && return 0
    _phase_banner "Neovim — LazyVim"

    pacman_batch "neovim" neovim

    if [[ ! -d "${HOME}/.config/nvim" ]]; then
        X git clone https://github.com/LazyVim/starter "${HOME}/.config/nvim"
        X rm -rf "${HOME}/.config/nvim/.git"
        _ok "LazyVim installed → run 'nvim' to install plugins"
    else
        _skip "~/.config/nvim (already exists)"
    fi

    _state_done "neovim"
    PHASE_TIMES[neovim]=$(( SECONDS - t ))
}

# ══════════════════════════════════════════════════════════════════════════════
#  FINAL REPORT
#  Per-phase timing table · network I/O · failures + remediation · next steps
# ══════════════════════════════════════════════════════════════════════════════
phase_report() {
    local total=$(( SECONDS - SCRIPT_START ))
    local net_str
    net_str="$(_net_delta)"

    printf "\n${BOLD}${MAGENTA}  ╔══════════════════════════════════════════════╗${RESET}\n"
    printf "${BOLD}${MAGENTA}  ║         INSTALLATION COMPLETE                ║${RESET}\n"
    printf "${BOLD}${MAGENTA}  ╚══════════════════════════════════════════════╝${RESET}\n\n"

    printf "  ${GREEN}${BOLD}✔ Installed${RESET}   %d packages\n"   "$INSTALL_COUNT"
    printf "  ${DIM}⊘ Skipped${RESET}    %d packages\n"            "${#SKIPPED_PKGS[@]}"
    printf "  ${RED}✘ Failed${RESET}     %d packages\n"            "${#FAILED_PKGS[@]}"
    printf "  ${CYAN}⏱ Total time${RESET}  %s\n"                    "$(_elapsed $total)"
    printf "  ${CYAN}🌐 Network${RESET}    %s\n"                     "$net_str"
    printf "  ${DIM}📄 Log${RESET}        %s\n\n"                    "$LOG_FILE"

    # Per-phase timing table
    if [[ ${#PHASE_TIMES[@]} -gt 0 ]]; then
        printf "  ${BOLD}Phase timing:${RESET}\n"
        # Sort by time descending (most expensive first)
        while IFS= read -r line; do
            printf "    %s\n" "$line"
        done < <(
            for phase in "${!PHASE_TIMES[@]}"; do
                printf "%4d  ${CYAN}%-22s${RESET} %s\n" \
                    "${PHASE_TIMES[$phase]}" \
                    "$phase" \
                    "$(_elapsed "${PHASE_TIMES[$phase]}")"
            done | sort -rn | sed 's/^[0-9]* //'
        )
        echo ""
    fi

    # Failures with remediation steps
    if [[ ${#FAILED_PKGS[@]} -gt 0 ]]; then
        printf "  ${RED}${BOLD}Failed packages + how to fix:${RESET}\n"
        for i in "${!FAILED_PKGS[@]}"; do
            printf "  ${RED}  ✘ %s${RESET}\n" "${FAILED_PKGS[$i]}"
            [[ -n "${FAILED_REMEDIATION[$i]:-}" ]] \
                && printf "${DIM}%s${RESET}\n" "${FAILED_REMEDIATION[$i]}"
        done
        printf '\n%s\n' "${FAILED_PKGS[@]}" > "${FAILED_LOG}"
        printf "\n  ${DIM}Retry failed installs:${RESET}\n"
        printf "  ${CYAN}  bash %s --%s --resume${RESET}\n\n" "$SCRIPT_NAME" "$MODE"
    fi

    # Run benchmark if requested
    _run_benchmark

    printf "  ${BOLD}Next steps:${RESET}\n"
    printf "  ${YELLOW}1.${RESET} ${BOLD}Reboot${RESET} — docker group + GPU drivers take effect\n"
    printf "  ${YELLOW}2.${RESET} ${CYAN}source ~/.zshrc${RESET}  or open a new terminal\n"
    printf "  ${YELLOW}3.${RESET} ${CYAN}pyenv install 3.12.3 && pyenv global 3.12.3${RESET}\n"
    printf "  ${YELLOW}4.${RESET} ${CYAN}fnm install --lts && fnm default lts-latest${RESET}\n"
    printf "  ${YELLOW}5.${RESET} ${CYAN}sdk install java${RESET}  — pick JVM version interactively\n"
    printf "  ${YELLOW}6.${RESET} ${CYAN}nvim${RESET}  — LazyVim installs plugins on first run\n"
    printf "  ${YELLOW}7.${RESET} ${CYAN}conda activate ml-base${RESET}  — AI/ML environment\n"
    printf "  ${YELLOW}8.${RESET} ${CYAN}ollama pull llama3${RESET}  — pull your first local model\n"
    printf "  ${YELLOW}9.${RESET} ${CYAN}rustup update${RESET}\n"
    printf "  ${YELLOW}10.${RESET} ${CYAN}aider --model claude-3-5-sonnet-20241022${RESET}  — start coding with AI\n\n"

    [[ $OPT_DRY -eq 1 ]] && printf "  ${MAGENTA}${BOLD}This was a DRY RUN — nothing was installed.${RESET}\n\n"
}

# ══════════════════════════════════════════════════════════════════════════════
#  PHASE DISPATCHER
# ══════════════════════════════════════════════════════════════════════════════
_run_phase() {
    case "$1" in
        preflight)        phase_preflight        ;;
        mirrors)          phase_mirrors           ;;
        base)             phase_base              ;;
        gpu)              phase_gpu               ;;
        fonts)            phase_fonts             ;;
        cli)              phase_cli               ;;
        sysutils)         phase_sysutils          ;;
        version_managers) phase_version_managers  ;;
        python)           phase_python            ;;
        rust)             phase_rust              ;;
        node)             phase_node              ;;
        dev_tools)        phase_dev_tools         ;;
        ml_stack)         phase_ml_stack          ;;
        shell)            phase_shell             ;;
        neovim)           phase_neovim            ;;
        report)           phase_report            ;;
        *) _fatal "Unknown phase: '$1'. Run --list-phases to see all." ;;
    esac
}

# ══════════════════════════════════════════════════════════════════════════════
#  SUDO KEEPALIVE
#  Problem: sudo credentials expire after 5 min (default). A 60-min --full run
#  hits ~10 expirations. Each one silently blocks waiting for a password that
#  never comes when the user has walked away.
#
#  Solution:
#    1. Prompt for sudo password exactly ONCE, right at startup, visibly.
#    2. Spawn a background loop that calls `sudo -n true` every 240s.
#       240s < 300s default TTL, giving 60s safety margin.
#       -n = non-interactive: silently fails rather than prompting.
#    3. Register the keepalive PID in BG_PIDS so _cleanup() kills it on exit.
# ══════════════════════════════════════════════════════════════════════════════
_sudo_keepalive() {
    [[ $OPT_DRY -eq 1 ]] && { _dim "[dry] skipping sudo keepalive"; return 0; }

    printf "\n${BOLD}${YELLOW}  ◆ sudo authentication${RESET}\n"
    printf "  ${DIM}Enter your password once — the script runs autonomously after this.${RESET}\n\n"

    sudo -v || _fatal "sudo authentication failed. Check your sudoers configuration."

    (
        while kill -0 "${SCRIPT_PID}" 2>/dev/null; do
            sudo -n true 2>/dev/null || true
            sleep 240
        done
    ) &
    local kpid=$!
    BG_PIDS+=("${kpid}")
    disown "${kpid}" 2>/dev/null || true
    _ok "sudo keepalive active — refreshes every 4 min (PID ${kpid})"
}

# ══════════════════════════════════════════════════════════════════════════════
#  MAIN
# ══════════════════════════════════════════════════════════════════════════════
main() {
    # Print log path first — before any work — so user knows where to look
    # even if the script immediately hangs on a prompt or preflight error.
    _print_log_header

    _parse_args "$@"

    # Authenticate sudo once upfront; background loop keeps it fresh forever.
    _sudo_keepalive

    _acquire_lock
    _net_snapshot

    # ── Single-phase run ──────────────────────────────────────────────────────
    if [[ -n "$OPT_PHASE" ]]; then
        [[ $OPT_DRY -eq 1 ]] && _warn "DRY RUN — nothing will be installed"
        TOTAL_PHASES=1
        phase_preflight
        _run_phase "$OPT_PHASE"
        phase_report
        return 0
    fi

    # ── Full mode run ─────────────────────────────────────────────────────────
    [[ -v "MODE_PHASES[$MODE]" ]] || _fatal "Invalid mode: '${MODE}'"

    [[ $OPT_FAST -eq 1 ]] && _enable_fast_pacman

    # +2: phase_preflight and phase_mirrors are called outside the MODE_PHASES
    # loop but both call _phase_banner, so they count toward the total.
    # Without +2, progress immediately exceeds 100% and the bar overflows.
    TOTAL_PHASES=$(( $(echo "${MODE_PHASES[$MODE]}" | wc -w) + 2 ))

    _confirm_start "$MODE"

    # Always run preflight + mirrors first (outside the mode map)
    phase_preflight
    phase_mirrors

    # Run mode phases in order
    for phase in ${MODE_PHASES[$MODE]}; do
        _run_phase "$phase"
    done

    # Post-run cleanup
    if [[ $OPT_DRY -eq 0 ]]; then
        _heartbeat_start "cleaning package cache"
        _spin_start "Cleaning package cache…"
        sudo paccache -rk2 &>/dev/null && true
        _spin_stop
        _heartbeat_stop
        _ok "Package cache cleaned (kept 2 versions)"

        command -v pipx &>/dev/null \
            && pipx upgrade-all &>/dev/null \
            && _ok "pipx tools upgraded"
    fi

    phase_report
}

main "$@"
