# Development Log (Chronological)

This file tracks the project chronologically, including requirement pivots, implementation direction changes, technical decisions, packaging changes, and commit message history.

Date context in this log uses the active project date window around **March 2, 2026**.

## 0) Pre-Git Requirement Evolution (Before Stable Direction)

1. Initial objective
- Build an autonomous software installer workflow with minimal/no user babysitting.
- Early concept started from a Bash-based approach.

2. Direction pivot: script -> TUI product
- Shifted from raw scripting UX toward a Rust terminal UI product.
- Constraint added: Arch Linux only.
- Requirement added: backend priority and fallback behavior (install from preferred source first, then fallback helpers).

3. UX/product expansion
- Keyboard-only navigation and operation.
- Fast package discovery (fuzzy find) and queue workflow.
- Bulk/manual package entry (examples like `fastfetch,btop,nvtop`).
- Autonomous install processing with reduced manual intervention.

4. Distribution + ops requirements
- Repository initialization and GitHub remote setup.
- SSH-based push workflow.
- Global invocation from any directory.
- Tarball distribution artifact.

5. Defect-driven iteration
- User-reported install behavior issue around `nvtop` path.
- Required: runtime fix + repush.

## 1) Baseline Code Audit (When Rewrite Started)

Workspace audited at `/home/user/install-script`.

Observed baseline:
- Old prototype architecture still active:
  - Queue/Browse split views
  - Decorative ASCII art panel
- Compiled successfully but did not match full radical-upgrade target architecture.

## 2) Major Convergence: Prototype -> Unified Architecture

### 2.1 UI architecture convergence
- Replaced view switching with unified, always-on main screen.
- Removed ASCII-art and animation subsystem from active architecture.
- Introduced modal behavior and pane focus concepts.

### 2.2 Backend abstraction convergence
- Replaced hardcoded chain behavior with backend abstraction layer.
- Added backend detection for:
  - `pacman`, `yay`, `paru`, `aura`, `trizen`
- Added runtime priority ordering behavior.

### 2.3 Data/cache convergence
- Introduced SQLite cache at:
  - `~/.cache/arch-package-tui/pkgdb.sqlite`
- Startup changed to cache-first rendering + background sync model.
- Added query-driven AUR integration path.

### 2.4 Matching/search convergence
- Replaced ad hoc fuzzy approach with `nucleo-matcher`.

### 2.5 Installer convergence
- Added autonomous fallback execution strategy across backend priority.
- Added preflight checks:
  - Arch check
  - non-root execution
  - internet TCP check on port 443
- Added dry-run path and queue action semantics.

### 2.6 CLI/headless convergence
- Added scriptable mode:
  - `--no-tui`
  - `--install`
  - `--remove`
  - `--load`

### 2.7 Distribution convergence
- Added global installation script (`install.sh`).
- Produced updated tarball artifacts.

## 3) Validation and Runtime Checks Performed

1. Compilation/quality
- `cargo fmt`
- `cargo check`

2. Headless behavior
- `cargo run -- --install nvtop --no-tui`
- Verified valid flow, including already-installed handling.

3. Global invocation
- Installed binary into `~/.local/bin/`
- Confirmed execution from outside repo path.

## 4) Docs and Repo Process Iteration

1. Development tracking
- Added `development.md` initially.
- Updated commit history section as repository evolved.

2. User verification cycle
- Verified branch sync state between local and GitHub remote.

## 5) 2026-03-02 Responsive Scalability Upgrade (This Round)

User request in this round:
- Ensure layout is scalable and undistorted.
- Update tarball to include latest changes.
- Add `prompt.md` with project and development guidance.
- Document detailed changes in `development.md`.

### 5.1 Implementation changes made

#### A) Responsive UI/undistorted layout rework
File: `src/ui.rs`

- Added explicit responsive layout breakpoints:
  - `Wide`: tri-pane horizontal layout
  - `Medium`: list + stacked detail/feed
  - `Narrow`: vertical stack list/detail/feed
- Added minimum-size fallback renderer (`render_minimal`) for very small terminals.
- Added compact warning behavior for small terminals (<80x24).
- Reworked header rendering to avoid clipping:
  - adaptive right-side mode/time/sync text
  - backend badge count truncation with `+N` overflow indicator
- Reworked package table rendering to avoid distortion:
  - dynamic table column schema via `TableSpec`
  - adaptive columns per width bucket
  - description budget scaling by width
- Improved activity feed text clipping per pane width.
- Reworked queue bar to prevent overflow:
  - dynamic tag capacity based on available width
  - adaptive suffix detail for small/medium/large widths
  - status text truncation
- Hardened overlay centering for small dimensions using clamped popup sizes.

#### B) Minor app cleanup
File: `src/app.rs`

- Removed now-unused `queue_preview_labels` method after queue bar moved to dynamic direct rendering logic.

#### C) Version and release metadata
File: `Cargo.toml`

- Version bump: `0.2.0` -> `0.2.1`

#### D) Maintainer prompt file
File: `prompt.md`

- Added high-signal project context and contributor guide:
  - architecture map
  - runtime model
  - responsiveness and layout behavior
  - command/keybind surface
  - build/test/release workflow
  - collaboration conventions/non-negotiables

### 5.2 Verification after responsive rework
- Ran `cargo fmt`
- Ran `cargo check`
- Confirmed successful compile after layout changes.

## 6) Packaging and Tarball Evolution

Existing historical tarballs:
- `dist/arch-package-tui-0.1.0.tar.gz`
- `dist/arch-package-tui-0.2.0.tar.gz`

This round target:
- Build and publish refreshed tarball containing responsive layout upgrade and new docs.

Finalized artifact in this round:
- `dist/arch-package-tui-0.2.1.tar.gz`

## 7) Git Remote and Branch State

Remote:
- `origin -> git@github.com:404Prabhat/install-tui.git`

Branch flow:
- `master` tracking `origin/master`

## 8) Commit Message History (Exact)

Commit history including this round:

1. `f8fc6adcf144b0eeb93259dee40807324ea5308b`
- Date: `2026-03-02 11:18:41 +0545`
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Rewrite into tri-pane Arch package TUI with async cache, fuzzy search, and headless mode`

2. `42b80c0c4d906af56786761f93ce3f89633e9f49`
- Date: `2026-03-02 11:24:53 +0545`
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Add chronological development log with pivots and commit history`

3. `ff620d940d4cd17c37a629a37b13dabfd702d657`
- Date: `2026-03-02 11:25:14 +0545`
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Update development log to include latest commit-history entries`

4. `0048e598fd4458feb0e5537f827d5111e1224c96`
- Date: `2026-03-02` (local commit time)
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Add responsive undistorted layout, maintainer prompt, docs updates, and v0.2.1 tarball`

5. `1e705a0...`
- Date: `2026-03-02` (local commit time)
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Sync development log with finalized responsive-upgrade commit and tarball`

6. `734dc54...`
- Date: `2026-03-02` (local commit time)
- Author: `Prabhat Aryal <404prabhat@gmail.com>`
- Message: `Refresh v0.2.1 tarball with latest development log`

Planned final sync commit message:
- `Finalize development log with latest commit history entries`

## 9) Direction-Change Summary

1. Bash-first installer concept -> Rust TUI product.
2. Split views + decorative art -> unified operational package commander.
3. Hardcoded manager behavior -> backend abstraction and configurable priority.
4. Blocking index load -> cache-first startup + async background sync.
5. Static UI assumptions -> responsive breakpoint-driven layout scaling.
6. Single interaction mode -> combined TUI + headless scriptable mode.
