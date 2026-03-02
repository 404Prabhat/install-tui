# Project Prompt / Maintainer Guide

This document is the high-signal prompt/context file for contributors and future agents working on `arch-package-tui`.

## 1) Project Identity

`arch-package-tui` is a keyboard-first, Arch Linux package commander built with Rust + Ratatui.

Core experience goals:
- Zero mouse dependency
- Fast package discovery + queueing
- Autonomous package actions (install/remove/upgrade)
- Reliable fallback across multiple backends
- Scriptable headless mode for automation

## 2) Platform and Scope

- Target OS: Arch Linux only
- Must fail safely on non-Arch systems
- Must run as non-root user
- Uses `sudo` only when backend command requires elevated permissions

## 3) Current Architecture

Main runtime modules:
- `src/main.rs`: CLI entrypoint, TUI bootstrap, `--no-tui` flow
- `src/app.rs`: state machine, key handling, command mode, events, queue logic
- `src/ui.rs`: all rendering, responsive layout breakpoints, overlays
- `src/backend.rs`: backend abstraction (`pacman`, `yay`, `paru`, `aura`, `trizen`)
- `src/installer.rs`: install/remove/full-upgrade execution with fallback chain
- `src/db.rs`: SQLite cache initialization and package persistence
- `src/syncer.rs`: background package DB sync + AUR query integration
- `src/detail.rs`: package detail fetch tasks
- `src/config.rs`: config and package set persistence
- `src/model.rs`: shared domain models/events/enums

## 4) Runtime Design

### 4.1 Modes
- `NORMAL`: navigation + operations
- `FILTER`: instant fuzzy filtering (`/`)
- `COMMAND`: `:` commands

### 4.2 Focus panes
- Package list
- Detail pane
- Activity feed

### 4.3 Responsive layout behavior
The renderer in `src/ui.rs` uses breakpoints:
- Wide: 3-column tri-pane
- Medium: list left + detail/feed stacked right
- Narrow: stacked vertical list/detail/feed

This prevents distortion and keeps controls usable across terminal sizes.

### 4.4 Minimum-size handling
- Full UI optimized for 80x24+
- Compact behavior below 80x24
- Explicit minimal fallback below 52x16

## 5) Data and Persistence

### 5.1 Cache DB
- Path: `~/.cache/arch-package-tui/pkgdb.sqlite`
- Holds package metadata and installed/upgradable flags

### 5.2 Config
- Path: `~/.config/arch-package-tui/config.toml`
- Includes backend priority and behavior toggles

### 5.3 Saved sets
- Path: `~/.config/arch-package-tui/sets.toml`
- Named package collections for reuse

### 5.4 Logs
- Directory: `~/.cache/arch-package-tui/`
- Install logs use `install-<timestamp>.log`

## 6) Command Surface

Normal mode highlights:
- `j/k`, arrows, `g/G`, `Enter`
- `i` install preview + start
- `r` removal mode
- `u` upgradable view, `U` full upgrade
- `s` sort cycle, `S` sync
- `d` remove from queue, `x` clear queue
- `t` dry-run toggle
- `?` help overlay

Command mode highlights:
- `:search <query>`
- `:install <pkg...>`
- `:remove <pkg...>`
- `:info <pkg>`
- `:sync`, `:upgrade`, `:orphans`, `:history`
- `:save <name>`, `:load <name>`
- `:q` / `:quit`

## 7) Build/Test/Release Workflow

### 7.1 Fast dev loop
```bash
cargo fmt
cargo check
```

### 7.2 Functional smoke test
```bash
cargo run -- --install nvtop --no-tui
```

### 7.3 Release build
```bash
cargo build --release
```

### 7.4 Local install
```bash
./install.sh
```

### 7.5 Tarball packaging pattern
1. Create `dist/arch-package-tui-<version>/`
2. Copy release binary and runtime docs/scripts
3. Create `dist/arch-package-tui-<version>.tar.gz`

## 8) Git and Collaboration Rules for This Repo

- Keep commits scoped and descriptive
- Update `development.md` whenever architecture or workflow changes
- Prefer preserving backwards-compatible keybind behavior where possible
- If adding/removing commands, update README + prompt.md + development.md
- Verify push status with:
  - `git status --short --branch`
  - `git rev-parse master && git rev-parse origin/master`

## 9) Non-Negotiables

- Do not reintroduce decorative/animated panes that reduce utility
- Keep startup path fast and non-blocking
- Avoid panics in runtime paths
- Preserve keyboard sovereignty for all critical actions
