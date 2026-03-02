# Development Log (Chronological)

This file documents the full development journey of `arch-package-tui` in chronological order, including major requirement pivots and git commit history.

## 0) Pre-Git Context and Requirement Evolution

This project direction changed multiple times before the current architecture stabilized.

1. Initial goal:
- Start with a Bash-based autonomous installer script for required software with no interactive babysitting.
- Inspiration target: Omarchy-style installer workflow and UX.

2. First major pivot:
- Direction changed from Bash script toward a Rust-based keyboard-first TUI.
- New requirement: Arch Linux only.
- New requirement: package source priority behavior (`pacman`, `yay`, `paru`) with fallback when a package is unavailable in higher-priority sources.

3. Expansion of UX goals:
- Full keyboard operation.
- Fast package browsing and fuzzy search.
- Queue-style install flow with manual package entry such as `fastfetch,btop,nvtop`.
- Autonomous install behavior for large package sets.
- Modern UI quality target and better navigation.

4. Additional product/ops requirements:
- Create repo in home directory.
- Configure GitHub SSH workflow.
- Push code to remote repository.
- Provide global invocation from any directory.
- Create distributable tarball.

5. Issue-driven pivot:
- A runtime issue was reported around `nvtop` installation behavior.
- Requested action: fix errors and push updated implementation.

## 1) Baseline Audit (Current Workspace State)

After entering `/home/user/install-script`, the repository was audited:

- Existing code was still the older prototype architecture (queue/browse view switching + ASCII art panel).
- Not yet on the full tri-pane/modal architecture requested in the radical upgrade specification.
- Existing code compiled (`cargo check`) but did not implement the final spec direction.

## 2) Convergence to New Architecture

A full rewrite was started from the prototype toward the final direction.

### 2.1 Core architecture convergence

- Replaced split-view mental model with a unified tri-pane interface.
- Removed ASCII art feature path and legacy indexer/view-switch assumptions.
- Introduced modal input model:
  - `NORMAL`
  - `FILTER` (`/`)
  - `COMMAND` (`:`)

### 2.2 Backend convergence

- Implemented backend abstraction for:
  - `pacman`
  - `yay`
  - `paru`
  - `aura`
  - `trizen`
- Added detection of available helpers and runtime backend priority stack.
- Added reordering behavior for backend priority in-app.

### 2.3 Data/cache convergence

- Added SQLite package cache at:
  - `~/.cache/arch-package-tui/pkgdb.sqlite`
- Startup behavior converged to:
  - load cached package data immediately
  - run background sync task asynchronously
- Added on-demand AUR search integration for query-driven augmentation.

### 2.4 Matching/search convergence

- Replaced ad-hoc fuzzy logic with `nucleo-matcher` scoring.
- Filtered/sorted visible package rows through scoring and sort modes.

### 2.5 Installer convergence

- Rebuilt install pipeline with autonomous fallback through backend priority.
- Added preflight checks for:
  - Arch-only environment
  - non-root execution
  - internet reachability via TCP 443 test
- Added dry-run behavior and queue-based install/remove actions.
- Added install feed event stream and completion summary path.

### 2.6 UX and command convergence

- Added command bar and command execution surface for:
  - `:search`
  - `:install`
  - `:remove`
  - `:info`
  - `:sync`
  - `:upgrade`
  - `:save`
  - `:load`
  - `:orphans`
  - `:history`
  - `:q`
- Added queue bar, activity feed, detail pane, help overlay, and preview overlay.

### 2.7 CLI/headless convergence

- Added no-TUI scripting mode:
  - `--no-tui`
  - `--install`
  - `--remove`
  - `--load`

## 3) Packaging, Invocation, and Distribution

1. Global invocation setup:
- Added installer script to place binary into `~/.local/bin`:
  - `install.sh`

2. Tarball output:
- Created updated distribution bundle:
  - `dist/arch-package-tui-0.2.0.tar.gz`

3. Invocation validation:
- Verified headless call from outside repo (`/tmp`) works.
- Verified `nvtop` flow in headless mode reached expected behavior.

## 4) Runtime Validation Notes

- Build/format/check performed.
- `cargo check` passed on rewritten architecture.
- Headless execution tested with `nvtop`; package resolution path worked and correctly handled already-installed case.

## 5) Git and Remote Convergence

- Remote configured:
  - `git@github.com:404Prabhat/install-tui.git`
- Branch tracking set:
  - `master -> origin/master`

## 6) Commit History (Exact Messages)

Below is the exact/declared commit history tracked during development logging.

1. `f8fc6adcf144b0eeb93259dee40807324ea5308b`
- Date: `2026-03-02 11:18:41 +0545`
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Rewrite into tri-pane Arch package TUI with async cache, fuzzy search, and headless mode`

2. `42b80c0...`
- Date: `2026-03-02` (local commit time)
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Add chronological development log with pivots and commit history`

3. `pending-at-edit-time`
- Date: `2026-03-02` (local commit time)
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Update development log to include latest commit-history entries`

## 7) Major Direction Changes Summary

1. Bash autonomous installer idea -> Rust TUI application.
2. Two-view prototype + decorative ASCII art -> unified tri-pane operational interface.
3. Hardcoded manager chain presets -> backend abstraction + reorderable priority.
4. Blocking index load behavior -> cache-first startup + background sync.
5. Manual-only interactive workflow -> combined TUI + scriptable headless interface.
