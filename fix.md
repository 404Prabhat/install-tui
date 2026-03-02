# arch_dev_setup.sh — Complete Fix Document
**Version targeting:** `v4.0.0` → `v4.1.0`  
**Purpose:** Make `bash arch_dev_setup.sh --full` run completely autonomously, with live progress, logging, and zero hangs.

---

## Table of Contents

1. [Root Cause Analysis — Why It Hangs for 10 Hours](#1-root-cause-analysis)
2. [BUG 1 — sudo credentials expire silently mid-run](#bug-1--sudo-credentials-expire-silently-mid-run)
3. [BUG 2 — Two blocking interactive reads halt autonomous execution](#bug-2--two-blocking-interactive-reads-halt-autonomous-execution)
4. [BUG 3 — Spinner is completely invisible (stdout is piped to tee)](#bug-3--spinner-is-completely-invisible)
5. [BUG 4 — TOTAL_PHASES is miscounted, progress bar shows wrong percentages](#bug-4--total_phases-is-miscounted)
6. [BUG 5 — set -euo pipefail kills script on legitimate non-zero exits in phase_node](#bug-5--set--euo-pipefail-kills-script-on-legitimate-non-zero-exits)
7. [BUG 6 — paru_batch parallel mode BG_PIDS cleanup is broken](#bug-6--paru_batch-parallel-mode-bg_pids-cleanup-is-broken)
8. [BUG 7 — cargo_batch parallel subshells cannot mutate parent arrays](#bug-7--cargo_batch-parallel-subshells-cannot-mutate-parent-arrays)
9. [BUG 8 — curl_install silently succeeds on dry-run even when command already exists](#bug-8--curl_install-silent-false-positive)
10. [BUG 9 — Log path is printed too late; user sees nothing for minutes](#bug-9--log-path-is-printed-too-late)
11. [ENHANCEMENT — Add --yes flag for fully unattended mode](#enhancement--add---yes-flag)
12. [ENHANCEMENT — Add --gpu-choice flag for pre-selecting GPU driver](#enhancement--add---gpu-choice-flag)
13. [ENHANCEMENT — Add ETA to phase banner](#enhancement--add-eta-to-phase-banner)
14. [ENHANCEMENT — Add heartbeat logging during long silent operations](#enhancement--add-heartbeat-logging)
15. [ENHANCEMENT — Add inline explanatory comments throughout](#enhancement--add-inline-comments)
16. [Complete Replacement Blocks — Copy-Paste Ready](#complete-replacement-blocks)

---

## 1. Root Cause Analysis

Running `bash arch_dev_setup.sh --full` with no extra flags hits **two fatal blockers** in sequence that can account for the entire 10-hour hang:

| # | Location | What blocks | How long it blocks |
|---|----------|-------------|-------------------|
| 1 | `_confirm_start()` → `_confirm()` line 629 | `read -r ans` waits for keyboard input | **Forever** |
| 2 | `phase_gpu()` line 772 | `read -r NVIDIA_CHOICE` waits for keyboard input | **Forever** |
| 3 | `sudo` inside any `pacman_batch` call | sudo TTL expires, waits for password | **Forever** |

The script architecture is otherwise sound. All three are fixable with targeted changes. Nothing else needs to be rewritten.

---

## BUG 1 — sudo credentials expire silently mid-run

### The Problem

`sudo` credentials have a default TTL of **5 minutes** on most Arch Linux installs. The `--full` run takes 60+ minutes. When sudo expires mid-run and the terminal is unattended, every call to `sudo pacman`, `sudo systemctl`, etc. silently blocks waiting for a password that never comes.

The `set -euo pipefail` at the top of the script will not help here — `sudo` waiting for input is not a failure, it's a read. The script just **freezes**.

### The Fix

Add `_sudo_keepalive()` and call it once at the very start of `main()`, before `_acquire_lock`. This does two things:
1. Forces a single password prompt right at the start of the run, visibly, before any work begins.
2. Spawns a silent background loop that calls `sudo -n true` every 240 seconds (sudo default TTL is 300s, so we refresh 60s before expiry).

### Exact Change — `main()` function (around line 1656)

**BEFORE:**
```bash
main() {
    _parse_args "$@"
    _acquire_lock
    _net_snapshot
```

**AFTER:**
```bash
main() {
    _parse_args "$@"
    _sudo_keepalive   # <<< ADD THIS — single password prompt, then auto-refresh
    _acquire_lock
    _net_snapshot
```

### New Function — Add BEFORE `main()` around line 1654

```bash
# ══════════════════════════════════════════════════════════════════════════════
#  SUDO KEEPALIVE
#  Problem: sudo credentials expire every 5 minutes by default. A 60-minute
#  --full run will hit at least 10 sudo expirations, each one silently blocking
#  the script waiting for a password nobody will type.
#
#  Solution:
#    1. Prompt for sudo password ONCE, right at startup, so the user knows
#       exactly when they need to interact. After this, no more prompts.
#    2. Spawn a background loop (disowned so it doesn't become a zombie) that
#       calls `sudo -n true` every 240 seconds. -n = non-interactive, so if
#       it somehow fails it fails silently. The 240s interval gives a 60s
#       safety margin before the 300s default TTL expires.
#    3. Register the keepalive PID in BG_PIDS so _cleanup() kills it on exit.
#
#  In dry-run mode we still do this — the user might still be prompted by
#  preflight checks that call sudo even in dry mode.
# ══════════════════════════════════════════════════════════════════════════════
_sudo_keepalive() {
    printf "\n${BOLD}${YELLOW}  ◆ sudo authentication${RESET}\n"
    printf "  ${DIM}Enter your password once — the script will run autonomously after this.${RESET}\n\n"

    # Validate sudo immediately. If this fails, abort before doing anything.
    sudo -v || _fatal "sudo authentication failed — cannot continue. Check sudoers config."

    # Background keepalive loop. Runs silently for the lifetime of this script.
    # kill -0 $SCRIPT_PID checks if our parent is still alive without sending a signal.
    (
        while kill -0 "${SCRIPT_PID}" 2>/dev/null; do
            # -n = non-interactive (never prompt). If TTL already expired somehow,
            # this fails silently — better than hanging.
            sudo -n true 2>/dev/null || true
            sleep 240
        done
    ) &

    local keepalive_pid=$!
    BG_PIDS+=("${keepalive_pid}")          # tracked for cleanup
    disown "${keepalive_pid}" 2>/dev/null || true

    _ok "sudo keepalive active — refreshes every 4 min (PID ${keepalive_pid})"
}
```

---

## BUG 2 — Two blocking interactive reads halt autonomous execution

### The Problem

Two functions perform `read` from stdin with no timeout:

**Location 1:** `_confirm_start()` → `_confirm()` at **line 629**
```bash
_confirm "Proceed with installation?" "y" || { printf "  Aborted.\n\n"; exit 0; }
```
`_confirm` calls `read -r ans` with no `-t` timeout. If the user launches the script and walks away, this blocks forever.

**Location 2:** `phase_gpu()` at **line 772**
```bash
printf "  Choice [1-4, default=1]: "
read -r NVIDIA_CHOICE
```
Same problem. Even if the user answers the first prompt, this one hits mid-run when they've walked away.

### The Fix

**Part A:** Add `OPT_YES=0` to runtime state (around line 93).

**Part B:** Modify `_confirm()` to skip the read when `OPT_YES=1`.

**Part C:** Modify `phase_gpu()` to use `NVIDIA_CHOICE` env var or `--gpu-choice` flag instead of reading stdin.

**Part D:** Add `--yes` to `_parse_args()`.

### Exact Change — Runtime State block (around line 93)

**BEFORE:**
```bash
MODE=""
OPT_FAST=0
OPT_DRY=0
OPT_RESUME=0
OPT_NO_REFLECTOR=0
OPT_BENCHMARK=0
OPT_LIST_PHASES=0
OPT_PHASE=""
NVIDIA_CHOICE=""
```

**AFTER:**
```bash
MODE=""
OPT_FAST=0
OPT_DRY=0
OPT_RESUME=0
OPT_NO_REFLECTOR=0
OPT_BENCHMARK=0
OPT_LIST_PHASES=0
OPT_PHASE=""
OPT_YES=0            # --yes / -y  → auto-answer all prompts (unattended mode)
OPT_GPU_CHOICE=""    # --gpu-choice 1-4 → pre-select NVIDIA driver at launch time
NVIDIA_CHOICE=""
```

### Exact Change — `_confirm()` function (around line 598)

**BEFORE:**
```bash
_confirm() {
    local prompt="$1" default="${2:-y}"
    local options
    if [[ "$default" == "y" ]]; then
        options="${GREEN}Y${RESET}/n"
    else
        options="y/${GREEN}N${RESET}"
    fi
    printf "  %s [%b] " "$prompt" "$options"
    local ans
    read -r ans
    ans="${ans:-$default}"
    [[ "${ans,,}" == "y" ]]
}
```

**AFTER:**
```bash
_confirm() {
    local prompt="$1" default="${2:-y}"

    # --yes flag: auto-answer all prompts with the default answer.
    # This enables fully unattended runs. Log what we're auto-accepting.
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
    # -t 30: timeout after 30 seconds, use default. Prevents permanent hang
    # even without --yes if the user walks away after starting.
    read -r -t 30 ans || ans="$default"
    ans="${ans:-$default}"
    [[ "${ans,,}" == "y" ]]
}
```

### Exact Change — `phase_gpu()` NVIDIA read block (around line 766)

**BEFORE:**
```bash
    # Auto-detect; prompt only when NVIDIA is present
    if lspci 2>/dev/null | grep -qi nvidia && [[ -z "$NVIDIA_CHOICE" ]]; then
        printf "\n  ${YELLOW}NVIDIA GPU detected.${RESET} Choose driver:\n\n"
        printf "    ${CYAN}1${RESET}) nvidia        — GTX 900+ / RTX series (proprietary)\n"
        printf "    ${CYAN}2${RESET}) nvidia-lts    — LTS kernel build\n"
        printf "    ${CYAN}3${RESET}) nvidia-open   — open kernel modules (RTX 20+)\n"
        printf "    ${CYAN}4${RESET}) Skip          — not using NVIDIA\n\n"
        printf "  Choice [1-4, default=1]: "
        read -r NVIDIA_CHOICE
        NVIDIA_CHOICE="${NVIDIA_CHOICE:-1}"
    fi
```

**AFTER:**
```bash
    # Auto-detect NVIDIA GPU and resolve driver choice.
    # Priority order:
    #   1. --gpu-choice N flag (set at launch time, fully non-interactive)
    #   2. NVIDIA_CHOICE env var (allows: NVIDIA_CHOICE=3 bash arch_dev_setup.sh --full)
    #   3. Interactive prompt with 30s timeout — falls back to choice 1 (proprietary)
    #   4. If --yes is set, skip the prompt entirely and use choice 1 as default
    if lspci 2>/dev/null | grep -qi nvidia && [[ -z "$NVIDIA_CHOICE" ]]; then

        # Use pre-set choice from flag or env var first
        if [[ -n "$OPT_GPU_CHOICE" ]]; then
            NVIDIA_CHOICE="$OPT_GPU_CHOICE"
            _info "GPU driver pre-selected via --gpu-choice: ${NVIDIA_CHOICE}"

        elif [[ $OPT_YES -eq 1 ]]; then
            # Unattended mode: default to proprietary driver (safest for most users)
            NVIDIA_CHOICE="1"
            _info "GPU driver auto-selected (--yes mode): nvidia proprietary (choice 1)"

        else
            printf "\n  ${YELLOW}NVIDIA GPU detected.${RESET} Choose driver:\n\n"
            printf "    ${CYAN}1${RESET}) nvidia        — GTX 900+ / RTX series (proprietary)\n"
            printf "    ${CYAN}2${RESET}) nvidia-lts    — LTS kernel build\n"
            printf "    ${CYAN}3${RESET}) nvidia-open   — open kernel modules (RTX 20+)\n"
            printf "    ${CYAN}4${RESET}) Skip          — not using NVIDIA\n\n"
            printf "  Choice [1-4, default=1, auto-selects in 30s]: "
            # -t 30: timeout after 30 seconds, default to 1 (proprietary driver)
            read -r -t 30 NVIDIA_CHOICE || true
            NVIDIA_CHOICE="${NVIDIA_CHOICE:-1}"
        fi
    fi
```

### Exact Change — `_parse_args()` (around line 569)

**BEFORE:**
```bash
        --no-reflector) OPT_NO_REFLECTOR=1   ;;
        --benchmark)    OPT_BENCHMARK=1      ;;
        --list-phases)  OPT_LIST_PHASES=1    ;;
        --phase)        OPT_PHASE="${2:-}";  [[ -z "$OPT_PHASE" ]] && _fatal "--phase requires a name"; shift ;;
        --clear-state)  _state_clear;        exit 0 ;;
        --help|-h)      _usage;              exit 0 ;;
        *) _fail "Unknown option: $1"; _usage; exit 1 ;;
```

**AFTER:**
```bash
        --no-reflector) OPT_NO_REFLECTOR=1   ;;
        --benchmark)    OPT_BENCHMARK=1      ;;
        --list-phases)  OPT_LIST_PHASES=1    ;;
        --yes|-y)       OPT_YES=1            ;;   # fully unattended mode
        --gpu-choice)   OPT_GPU_CHOICE="${2:-}";
                        [[ -z "$OPT_GPU_CHOICE" ]] && _fatal "--gpu-choice requires a value (1-4)";
                        [[ ! "$OPT_GPU_CHOICE" =~ ^[1-4]$ ]] && _fatal "--gpu-choice must be 1, 2, 3, or 4";
                        shift ;;
        --phase)        OPT_PHASE="${2:-}";  [[ -z "$OPT_PHASE" ]] && _fatal "--phase requires a name"; shift ;;
        --clear-state)  _state_clear;        exit 0 ;;
        --help|-h)      _usage;              exit 0 ;;
        *) _fail "Unknown option: $1"; _usage; exit 1 ;;
```

---

## BUG 3 — Spinner is completely invisible

### The Problem

Line 121 redirects all stdout through `tee`:
```bash
exec > >(tee -a "${LOG_FILE}") 2>&1
```

After this line, stdout is no longer a TTY — it's a pipe to the tee process. The spinner checks `[[ -t 1 ]]` (is fd 1 a terminal?) which now returns **false**, so `_spin_start()` returns immediately at line 157 and the spinner subshell is never launched. Every `_spin_start` call throughout the script silently does nothing.

The same `tee` redirect means ANSI escape codes for cursor movement get written to the log file as literal escape sequences, polluting it.

### The Fix

Write spinner output directly to `/dev/tty` (the actual terminal device), bypassing the `tee` redirect entirely. Check `[[ -e /dev/tty ]]` instead of `[[ -t 1 ]]`. The spinner's `ERASE` and `CR` sequences go to the terminal only, never to the log.

### Exact Change — `_spin_start()` and `_spin_stop()` (lines 156–179)

**BEFORE:**
```bash
_spin_start() {
    [[ -t 1 ]] || return 0
    local msg="${1:-Working…}"
    (
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
    printf "${ERASE}${CR}${SHOW_CURSOR}"
}
```

**AFTER:**
```bash
_spin_start() {
    # Spinner must write directly to /dev/tty because stdout is redirected to
    # tee (for logging). After `exec > >(tee ...)`, [[ -t 1 ]] is always false.
    # Writing to /dev/tty bypasses the tee pipe and writes directly to the
    # terminal. ANSI cursor/erase codes never appear in the log file this way.
    [[ -e /dev/tty ]] || return 0
    local msg="${1:-Working…}"
    (
        exec > /dev/tty 2>&1   # this subshell writes directly to the terminal
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
    # Write clear sequence directly to terminal, not to the log
    [[ -e /dev/tty ]] && printf "${ERASE}${CR}${SHOW_CURSOR}" > /dev/tty || true
}
```

---

## BUG 4 — TOTAL_PHASES is miscounted

### The Problem

In `main()` at line 1676:
```bash
TOTAL_PHASES=$(echo "${MODE_PHASES[$MODE]}" | wc -w)
```

`MODE_PHASES[full]` = `"base gpu fonts cli sysutils version_managers python rust node dev_tools ml_stack shell neovim"` — that's **13 phases**.

But `main()` then calls `phase_preflight` and `phase_mirrors` separately (outside the loop), and both of them call `_phase_banner` which increments `CURRENT_PHASE`. So the actual total is **15**.

With `TOTAL_PHASES=13`, the progress bar immediately shows percentages > 100% by the time the real phases start. The bar calculation `$(( CURRENT_PHASE * 100 / TOTAL_PHASES ))` becomes `$(( 15 * 100 / 13 ))` = 115%, and `$(( pct / 5 ))` = 23, which overflows the 20-char bar and causes the fill loop to write 23 `█` characters into a 20-slot bar — breaking the visual completely.

### The Fix

Count `preflight` and `mirrors` in `TOTAL_PHASES`.

### Exact Change — `main()` around line 1676

**BEFORE:**
```bash
    # Count total phases for progress bar
    TOTAL_PHASES=$(echo "${MODE_PHASES[$MODE]}" | wc -w)
```

**AFTER:**
```bash
    # Count total phases for progress bar.
    # +2 accounts for phase_preflight and phase_mirrors which are called
    # separately from the MODE_PHASES loop but still call _phase_banner.
    TOTAL_PHASES=$(( $(echo "${MODE_PHASES[$MODE]}" | wc -w) + 2 ))
```

Also fix the bar overflow in `_phase_banner()` with a clamp:

### Exact Change — `_phase_banner()` (around line 140)

**BEFORE:**
```bash
_phase_banner() {
    local name="$1"
    CURRENT_PHASE=$(( CURRENT_PHASE + 1 ))
    local pct=$(( CURRENT_PHASE * 100 / TOTAL_PHASES ))
    local filled=$(( pct / 5 ))
    local bar="" i
    for ((i=0; i<filled; i++));    do bar+="█"; done
    for ((i=filled; i<20; i++)); do bar+="░"; done
    printf "\n${BOLD}${MAGENTA}  ══ PHASE %d/%d  %-30s${RESET}\n" \
        "$CURRENT_PHASE" "$TOTAL_PHASES" "$name"
    printf "  ${CYAN}[%s] %d%%${RESET}\n\n" "$bar" "$pct"
}
```

**AFTER:**
```bash
_phase_banner() {
    local name="$1"
    CURRENT_PHASE=$(( CURRENT_PHASE + 1 ))

    # Guard against TOTAL_PHASES=0 (division by zero) and clamp pct to 100
    # in case CURRENT_PHASE overshoots TOTAL_PHASES due to miscounting.
    local pct=0
    if (( TOTAL_PHASES > 0 )); then
        pct=$(( CURRENT_PHASE * 100 / TOTAL_PHASES ))
        (( pct > 100 )) && pct=100
    fi

    # Bar is 20 chars wide. filled = number of filled slots.
    # Clamp filled to [0,20] to prevent buffer overrun in the loop.
    local filled=$(( pct / 5 ))
    (( filled > 20 )) && filled=20
    (( filled < 0  )) && filled=0

    local bar="" i
    for ((i=0; i<filled; i++));    do bar+="█"; done
    for ((i=filled; i<20; i++)); do bar+="░"; done

    # ETA: calculate remaining time based on elapsed time per completed phase
    local elapsed=$(( SECONDS - SCRIPT_START ))
    local eta_str=""
    if (( CURRENT_PHASE > 1 && elapsed > 0 )); then
        local remaining=$(( TOTAL_PHASES - CURRENT_PHASE ))
        local secs_per_phase=$(( elapsed / (CURRENT_PHASE - 1) ))
        local eta_secs=$(( remaining * secs_per_phase ))
        eta_str="  ETA ~$(_elapsed ${eta_secs})"
    fi

    printf "\n${BOLD}${MAGENTA}  ══ PHASE %d/%d  %-30s${RESET}\n" \
        "$CURRENT_PHASE" "$TOTAL_PHASES" "$name"
    printf "  ${CYAN}[%s] %d%%  elapsed: %s%s${RESET}\n\n" \
        "$bar" "$pct" "$(_elapsed ${elapsed})" "$eta_str"
}
```

---

## BUG 5 — set -euo pipefail kills script on legitimate non-zero exits

### The Problem

In `phase_node()`, the pnpm/bun/deno install blocks use a one-liner pattern that is broken under `set -e`:

```bash
command -v pnpm &>/dev/null \
    || X sh -c 'curl -fsSL https://get.pnpm.io/install.sh | sh -' \
    && _ok "pnpm"
```

This is parsed as: `(command -v pnpm || X sh ...) && _ok "pnpm"`

When `pnpm` is **not** installed and `X sh -c 'curl ...'` **fails** (e.g. network blip):
- `command -v pnpm` → exit 1
- `|| X sh ...` → runs, fails, exit 1
- The overall `||` expression exits 1
- `&& _ok "pnpm"` is skipped
- Final expression exits **1**
- `set -e` sees exit code 1 from a simple command → **script aborts**

Additionally, `_ok "pnpm"` prints a success message even when the tool was already installed (skipped), conflating install and skip in the log.

### The Fix

Replace the fragile one-liners with explicit `if/else` blocks that properly handle both the skip and failure paths.

### Exact Change — `phase_node()` installs (around lines 1092–1106)

**BEFORE:**
```bash
    _section "pnpm (disk-efficient, fast package manager)"
    command -v pnpm &>/dev/null \
        || X sh -c 'curl -fsSL https://get.pnpm.io/install.sh | sh -' \
        && _ok "pnpm"

    _section "bun (fast JS runtime + bundler)"
    command -v bun &>/dev/null \
        || X sh -c 'curl -fsSL https://bun.sh/install | bash' \
        && _ok "bun"

    _section "deno (secure, modern JS/TS runtime)"
    command -v deno &>/dev/null \
        || X sh -c 'curl -fsSL https://deno.land/install.sh | sh' \
        && _ok "deno"
```

**AFTER:**
```bash
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
```

---

## BUG 6 — paru_batch parallel mode BG_PIDS cleanup is broken

### The Problem

In `paru_batch()` with `OPT_FAST=1` (around line 351):

```bash
if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
    wait "${pids[0]}" 2>/dev/null || true
    pids=("${pids[@]:1}")
    BG_PIDS=("${BG_PIDS[@]/$pids[0]}")   # ← BUG: this uses pids[0] AFTER the slice
fi
```

After `pids=("${pids[@]:1}")`, `pids[0]` is now the **second** original element, not the one we waited for. So `BG_PIDS` removes the wrong PID. Over a long run with many AUR packages, `BG_PIDS` accumulates stale/wrong PIDs. `_cleanup()` then tries to `kill` these wrong PIDs, potentially killing unrelated processes.

### The Fix

Save `pids[0]` before slicing the array.

### Exact Change — `paru_batch()` (around line 351)

**BEFORE:**
```bash
        if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
            wait "${pids[0]}" 2>/dev/null || true
            pids=("${pids[@]:1}")
            BG_PIDS=("${BG_PIDS[@]/$pids[0]}")
        fi
```

**AFTER:**
```bash
        if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
            # Save the PID we're about to wait for BEFORE slicing the array.
            # After pids=("${pids[@]:1}"), pids[0] points to a different element.
            local finished_pid="${pids[0]}"
            wait "${finished_pid}" 2>/dev/null || true
            pids=("${pids[@]:1}")
            # Remove the finished PID from BG_PIDS using the saved value.
            BG_PIDS=("${BG_PIDS[@]/${finished_pid}}")
        fi
```

---

## BUG 7 — cargo_batch parallel subshells cannot mutate parent arrays

### The Problem

In `cargo_batch()` with `OPT_FAST=1` (around line 425):

```bash
(
    cargo install --list 2>/dev/null | grep -q "^${ct} " \
        && { SKIPPED_PKGS+=("$ct"); exit 0; }
    X cargo install "$ct" \
        && printf "${GREEN}  ✔${RESET}  %s (cargo)\n" "$ct" \
        || printf "${RED}  ✘${RESET}  %s (cargo)\n" "$ct"
) &
```

The subshell `()` creates a new process. Any changes to `SKIPPED_PKGS`, `FAILED_PKGS`, `INSTALL_COUNT`, etc. inside the subshell are **completely invisible to the parent**. The skip and fail tracking does nothing in parallel mode.

This means the final report's skipped/failed/installed counts are **wrong** in `--fast` mode.

### The Fix

Remove the array mutations from the subshell entirely. In parallel mode, accept that individual package tracking is imprecise — only log pass/fail to stdout. The parent already captures all output via `tee`. Keep accurate counts only in sequential mode.

### Exact Change — `cargo_batch()` parallel block (around line 423)

**BEFORE:**
```bash
    if [[ $OPT_FAST -eq 1 ]]; then
        local -a pids=() ct
        for ct in "$@"; do
            (
                cargo install --list 2>/dev/null | grep -q "^${ct} " \
                    && { SKIPPED_PKGS+=("$ct"); exit 0; }
                X cargo install "$ct" \
                    && printf "${GREEN}  ✔${RESET}  %s (cargo)\n" "$ct" \
                    || printf "${RED}  ✘${RESET}  %s (cargo)\n" "$ct"
            ) &
            pids+=($!)
            BG_PIDS+=($!)
            if (( ${#pids[@]} >= MAX_PARALLEL_AUR )); then
                wait "${pids[0]}" 2>/dev/null || true
                pids=("${pids[@]:1}")
            fi
        done
        wait "${pids[@]}" 2>/dev/null || true
```

**AFTER:**
```bash
    if [[ $OPT_FAST -eq 1 ]]; then
        # NOTE: Parallel subshells cannot write to parent-process arrays.
        # SKIPPED_PKGS / FAILED_PKGS / INSTALL_COUNT are NOT updated in this
        # path. Counts in the final report will be approximate in --fast mode.
        # All output is still captured to the log via the tee redirect.
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
```

---

## BUG 8 — curl_install silent false positive

### The Problem

`curl_install()` checks `[[ -d "${HOME}/.${check_cmd}" ]]` or `[[ -d "${HOME}/${check_cmd}" ]]` to determine if something is already installed. For tools like `uv`, `mise`, `fnm` — none of which install to a predictably named dotfolder — this check fails, and the function falls through to the curl install every time, even if the tool is already present. Meanwhile, `command -v "$check_cmd"` usually catches it. The issue is the directory check is overly broad and produces false positives for common names.

### The Fix

Tighten the idempotency check: only check the command, not generic dotfolders. Add an explicit message when skipping.

### Exact Change — `curl_install()` (around line 369)

**BEFORE:**
```bash
curl_install() {
    local label="$1" check_cmd="$2" url="$3"
    shift 3
    local extra_args=("$@")

    if command -v "$check_cmd" &>/dev/null \
        || [[ -d "${HOME}/.${check_cmd}" ]] \
        || [[ -d "${HOME}/${check_cmd}" ]]; then
        _skip "${label}"
        return 0
    fi
```

**AFTER:**
```bash
curl_install() {
    local label="$1" check_cmd="$2" url="$3"
    shift 3
    local extra_args=("$@")

    # Idempotency check: prefer command availability over directory guessing.
    # Directory checks (".${check_cmd}", "${check_cmd}") are kept as fallback
    # for installers that don't put themselves on PATH immediately (e.g. sdkman).
    if command -v "$check_cmd" &>/dev/null; then
        _skip "${label} ($(${check_cmd} --version 2>/dev/null | head -1 || echo 'installed'))"
        SKIPPED_PKGS+=("${label}")
        return 0
    elif [[ -d "${HOME}/.${check_cmd}" ]] || [[ -d "${HOME}/${check_cmd}" ]]; then
        _skip "${label} (directory exists — may need shell reload to appear on PATH)"
        SKIPPED_PKGS+=("${label}")
        return 0
    fi
```

---

## BUG 9 — Log path is printed too late

### The Problem

The log file is set up at line 121 with `exec > >(tee -a "${LOG_FILE}")`, but the log path is only mentioned inside `_confirm_start()` — which is called after preflight checks. The user running `--full` for the first time has no idea where to look for activity during the long preflight + mirror phase.

More critically: if the script hangs at the `_confirm_start` prompt with no terminal output, the user has no information and cannot easily find the log to debug.

### The Fix

Print the log path as the very first thing the script outputs — before `_parse_args`, before any work. Add a `tail -f` hint for monitoring in a second terminal.

### Exact Change — Add `_print_log_header()` function and call it

**Add this new function** immediately after `_init_colors` call (around line 88):

```bash
# ══════════════════════════════════════════════════════════════════════════════
#  LOG HEADER  — printed immediately on startup, before any work begins.
#  The user needs to know where to look BEFORE the script potentially hangs.
#  This also appears in the log itself (tee is set up before this call).
# ══════════════════════════════════════════════════════════════════════════════
_print_log_header() {
    printf "\n${BOLD}${CYAN}  ┌────────────────────────────────────────────────────┐${RESET}\n"
    printf "${BOLD}${CYAN}  │  arch-dev-setup v%-4s                               │${RESET}\n" "$SCRIPT_VERSION"
    printf "${BOLD}${CYAN}  │  📋 Log file:                                       │${RESET}\n"
    printf "${BOLD}${CYAN}  │    ${YELLOW}%-50s${CYAN}│${RESET}\n" "${LOG_FILE}"
    printf "${BOLD}${CYAN}  │  Monitor live in another terminal:                  │${RESET}\n"
    printf "${BOLD}${CYAN}  │    ${YELLOW}tail -f %-43s${CYAN}│${RESET}\n" "${LOG_FILE}"
    printf "${BOLD}${CYAN}  └────────────────────────────────────────────────────┘${RESET}\n\n"
}
```

**Then modify `main()`** to call it first:

**BEFORE:**
```bash
main() {
    _parse_args "$@"
    _sudo_keepalive
```

**AFTER:**
```bash
main() {
    _print_log_header   # always first — user needs log path before anything else
    _parse_args "$@"
    _sudo_keepalive
```

---

## ENHANCEMENT — Add --yes flag

Already covered in Bug 2. Update the `_usage()` function to document it:

### Exact Change — `_usage()` FLAGS section (around line 522)

**BEFORE:**
```bash
  ${BOLD}FLAGS${RESET}
    ${YELLOW}--fast${RESET}           ParallelDownloads + parallel AUR + parallel cargo
    ${YELLOW}--dry-run${RESET}        Print every action; install nothing
    ${YELLOW}--resume${RESET}         Skip phases already completed (reads state file)
    ${YELLOW}--no-reflector${RESET}   Skip mirror refresh (use 24 h cache or current list)
```

**AFTER:**
```bash
  ${BOLD}FLAGS${RESET}
    ${YELLOW}--yes, -y${RESET}        Unattended mode — auto-answer all prompts with defaults
    ${YELLOW}--gpu-choice N${RESET}   Pre-select GPU driver: 1=nvidia 2=lts 3=open 4=skip
    ${YELLOW}--fast${RESET}           ParallelDownloads + parallel AUR + parallel cargo
    ${YELLOW}--dry-run${RESET}        Print every action; install nothing
    ${YELLOW}--resume${RESET}         Skip phases already completed (reads state file)
    ${YELLOW}--no-reflector${RESET}   Skip mirror refresh (use 24 h cache or current list)
```

**And add the recommended launch command to the EXAMPLES section:**
```bash
  ${BOLD}EXAMPLES${RESET}
    bash ${SCRIPT_NAME} --full --fast --yes --gpu-choice 3     # fully autonomous
    bash ${SCRIPT_NAME} --dev --fast
    bash ${SCRIPT_NAME} --ml --fast --resume
    bash ${SCRIPT_NAME} --phase rust --dry-run
    bash ${SCRIPT_NAME} --dev --fast --no-reflector --resume
    bash ${SCRIPT_NAME} --benchmark --phase cli
```

---

## ENHANCEMENT — Add ETA to phase banner

Already included in the Bug 4 fix above within `_phase_banner()`. No additional changes needed.

---

## ENHANCEMENT — Add heartbeat logging

### The Problem

During long silent operations like `sudo pacman -Syu` (system upgrade) or PyTorch installation inside conda, the log file goes silent for 5–20 minutes. There is no way to tell if the script is working or frozen.

### The Fix

Add a `_heartbeat_start()` / `_heartbeat_stop()` pair that writes a timestamp to the log every 30 seconds during long-running operations. The heartbeat writes only to the log (not the terminal), so it doesn't interfere with spinner or progress output.

### New Functions — Add after `_spin_stop()` (around line 179)

```bash
# ══════════════════════════════════════════════════════════════════════════════
#  HEARTBEAT LOGGER
#  During silent long-running operations (system upgrades, PyTorch downloads,
#  cargo compilations) the log goes silent. Without a heartbeat, there is no
#  way to distinguish "working" from "frozen" when monitoring the log.
#
#  _heartbeat_start <msg>: begins logging "still working…" every 30s to log
#  _heartbeat_stop: kills the heartbeat background job
#
#  The heartbeat writes to the log file directly, not stdout, so it doesn't
#  interfere with spinner display or terminal output.
# ══════════════════════════════════════════════════════════════════════════════
HEARTBEAT_PID=""

_heartbeat_start() {
    local msg="${1:-working…}"
    (
        local count=0
        while true; do
            sleep 30
            count=$(( count + 30 ))
            printf "  [heartbeat] %s — still %s (%ds elapsed)\n" \
                "$(date '+%H:%M:%S')" "$msg" "$count" >> "${LOG_FILE}"
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
```

Also update `_cleanup()` to kill heartbeat:

### Exact Change — `_cleanup()` (around line 205)

**BEFORE:**
```bash
_cleanup() {
    local exit_code=${1:-$?}
    _spin_stop
```

**AFTER:**
```bash
_cleanup() {
    local exit_code=${1:-$?}
    _spin_stop
    _heartbeat_stop
```

### Usage — wrap long operations with heartbeat

In `phase_base()` around line 725:
```bash
    _heartbeat_start "running pacman -Syu"
    _spin_start "Full system upgrade…"
    X sudo pacman -Syu --noconfirm
    _spin_stop
    _heartbeat_stop
    _ok "System up to date"
```

In `phase_ml_stack()` around line 1253:
```bash
    _heartbeat_start "installing PyTorch (large download)"
    _spin_start "Installing PyTorch…"
    X "$conda_exe" run -n ml-base conda install -y \
        ...
    _spin_stop
    _heartbeat_stop
```

In `phase_ml_stack()` around line 1264:
```bash
    _heartbeat_start "pip installing ML libraries"
    _spin_start "Installing ML libraries (this takes a while)…"
    X "$conda_exe" run -n ml-base pip install --quiet \
        ...
    _spin_stop
    _heartbeat_stop
```

---

## ENHANCEMENT — Add inline comments

The following sections have non-obvious logic that needs inline documentation for the maintainer. Add these comments at the specified locations:

### `set -euo pipefail` + `IFS` (around line 46)

**BEFORE:**
```bash
set -euo pipefail
IFS=$'\n\t'
```

**AFTER:**
```bash
# -e  errexit:  exit immediately if any command exits with non-zero status,
#               unless the command is part of an if/while/until condition,
#               or followed by || or &&.
# -u  nounset:  treat unset variables as errors. Prevents silent bugs from
#               typos in variable names (e.g. $NVIDIA_CHIOCE vs $NVIDIA_CHOICE).
# -o pipefail:  if any command in a pipe fails, the whole pipe fails.
#               Without this, `false | true` exits 0 (masking the false).
set -euo pipefail

# Remove space from IFS (Internal Field Separator).
# Default IFS includes space, which causes word-splitting on unquoted
# variable expansions containing spaces. Keeping newline+tab is enough
# for array iteration while preventing accidental splitting on filenames
# with spaces (common in $HOME paths on some systems).
IFS=$'\n\t'
```

### `_state_done` / `_state_check` (around line 232)

**BEFORE:**
```bash
_state_done()  { echo "$1" >> "${STATE_FILE}"; }
_state_check() { grep -qxF "$1" "${STATE_FILE}" 2>/dev/null; }
_state_clear() { rm -f "${STATE_FILE}"; _ok "State cleared — full reinstall on next run"; }
```

**AFTER:**
```bash
# Resume state: append-only flat file. Each line = one completed phase name.
# _state_done "rust"  → appends "rust\n" to the state file
# _state_check "rust" → greps for exact line match (-x = full line, -F = literal)
# _state_clear        → deletes the file, forcing full reinstall on next run
#
# This design is intentionally simple: no JSON, no timestamps, no locking.
# Concurrent writes from background jobs are safe because bash's echo is
# atomic for short strings on Linux (single write() syscall < PIPE_BUF).
_state_done()  { echo "$1" >> "${STATE_FILE}"; }
_state_check() { grep -qxF "$1" "${STATE_FILE}" 2>/dev/null; }
_state_clear() { rm -f "${STATE_FILE}"; _ok "State cleared — full reinstall on next run"; }
```

### `exec > >(tee …)` (around line 121)

**BEFORE:**
```bash
exec > >(tee -a "${LOG_FILE}") 2>&1
```

**AFTER:**
```bash
# Tee all stdout and stderr to the log file for the remainder of the script.
# `exec > >(tee -a FILE)` redirects stdout to a process substitution that
# runs `tee -a FILE` in a background subshell. -a = append (not overwrite).
# `2>&1` then redirects stderr to the same (now-piped) stdout.
#
# IMPORTANT SIDE EFFECT: After this line, `[[ -t 1 ]]` returns false because
# stdout is now a pipe, not a terminal. Any code that checks for a TTY must
# use `[[ -e /dev/tty ]]` or write directly to /dev/tty instead.
exec > >(tee -a "${LOG_FILE}") 2>&1
```

### `pacman_batch` (around line 287)

**BEFORE:**
```bash
# ── pacman_batch ──────────────────────────────────────────────────────────────
#  Single pacman -S call for all missing packages in a group.
#  This is the primary speed gain vs. per-package calls.
```

**AFTER:**
```bash
# ── pacman_batch ──────────────────────────────────────────────────────────────
# Installs a group of pacman packages in a SINGLE pacman -S call.
#
# Why batch? pacman has significant per-invocation overhead (lock acquire,
# dependency resolution, DB sync). Installing 20 packages one-by-one takes
# ~10× longer than one call with all 20. This is the biggest speed win in
# the entire script for pacman-sourced packages.
#
# Idempotency: we pre-filter with `pacman -Qi` to find only missing packages
# and skip the pacman call entirely if everything is already installed.
# `pacman -Qi` is local-DB only (no network) and very fast.
#
# RETRY_ATTEMPTS (default 2): retries the entire batch on transient failures
# like network timeouts or mirror issues. 3-second sleep between attempts.
```

### `paru_batch` parallel block (around line 349)

```bash
    # Parallel AUR installs (--fast mode only).
    # WARNING: Parallel AUR builds can conflict if two packages share a
    # makedepend (both try to install the same dep simultaneously). This is
    # rare but can cause makepkg failures. MAX_PARALLEL_AUR=4 is conservative
    # enough to reduce this risk while still providing ~3× speedup on modern CPUs.
    # If you see random makepkg failures, reduce MAX_PARALLEL_AUR to 2 or 1.
```

---

## Complete Replacement Blocks

Below are the full replacement function bodies, ready to copy-paste directly into the script. These consolidate all fixes from the sections above.

---

### Full Replacement: Runtime State block (lines 93–115)

```bash
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
OPT_YES=0            # --yes / -y  → auto-answer all prompts, fully unattended
OPT_GPU_CHOICE=""    # --gpu-choice 1-4 → pre-select GPU driver at launch time
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
```

---

### Full Replacement: `_parse_args()` case block (lines 572–591)

```bash
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
            --phase)
                OPT_PHASE="${2:-}"
                [[ -z "$OPT_PHASE" ]] && _fatal "--phase requires a name"
                shift ;;
            --clear-state)  _state_clear; exit 0 ;;
            --help|-h)      _usage;       exit 0 ;;
            *) _fail "Unknown option: $1"; _usage; exit 1 ;;
        esac
        shift
    done
```

---

### Full Replacement: `main()` (lines 1655–1704)

```bash
# ══════════════════════════════════════════════════════════════════════════════
#  MAIN
# ══════════════════════════════════════════════════════════════════════════════
main() {
    # Print log path immediately — before any work — so user knows where to look
    # even if the script hangs on a prompt or error right at startup.
    _print_log_header

    _parse_args "$@"

    # Authenticate sudo once, upfront. After this, a background loop keeps
    # credentials fresh every 4 minutes so no sudo prompts appear mid-run.
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

    # +2 accounts for phase_preflight and phase_mirrors which are called
    # outside the MODE_PHASES loop but still call _phase_banner.
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
```

---

## Final Summary — Recommended Launch Command

After applying all fixes, the fully autonomous command is:

```bash
bash arch_dev_setup.sh --full --fast --yes --gpu-choice 3
```

What each flag does:
- `--full` — all 15 phases including ML stack
- `--fast` — parallel downloads, parallel AUR builds, parallel cargo compiles
- `--yes` — auto-answer every prompt with its default (no keyboard needed after start)
- `--gpu-choice 3` — select `nvidia-open` (RTX 20+); change to `1` for proprietary, `4` to skip GPU entirely

The script will:
1. Print the log file path immediately
2. Ask for your sudo password exactly once
3. Show a live spinner + phase progress bar with ETA at each phase
4. Write heartbeat timestamps to the log every 30s during long silent operations
5. Run completely unattended for 60–90 minutes
6. Print a full timing/failure report at the end

Monitor progress in a second terminal at any time:
```bash
tail -f ~/.cache/arch-setup/setup-$(ls ~/.cache/arch-setup/ | grep setup | tail -1 | cut -d/ -f1)
# or just:
tail -f ~/.cache/arch-setup/$(ls -t ~/.cache/arch-setup/ | head -1)
```
