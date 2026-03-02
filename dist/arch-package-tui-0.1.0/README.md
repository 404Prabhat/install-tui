# Arch Package TUI

A Rust-based, keyboard-first installer TUI for Arch Linux.

## Built for your requested workflow

- Start from a full TUI (no script prompts)
- Fuzzy-search package database (official + AUR index when helper exists)
- Add packages by manual input like:
  - `fastfetch,btop,nvtop`
  - or `fastfetch btop nvtop`
- Queue packages and install autonomously
- Source priority chain presets (default is `pacman -> yay -> paru`)
- Intelligent fallback install engine:
  - tries your chosen source order
  - batches installs for speed
  - recursively isolates failures so one bad package does not stop everything
- Live progress + logs
- Dynamic matrix-style ASCII art panel that rotates style every 10 seconds

## Arch-only

This tool expects Arch Linux (`/etc/arch-release`) and is intended for Arch package ecosystems.

## Run from source

```bash
cargo run
```

## Keyboard controls

- Global
  - `1` queue view
  - `2` browse view
  - `i` start install
  - `q` quit (or request abort during install)

- Queue view
  - `Up/Down` move focus between input/priority/queue/actions
  - Manual input: type package list, `Enter` or `a` to add
  - Priority row: `Left/Right` cycle source priority presets
  - `t` toggle dry-run
  - Queue list: `d` remove selected, `x` clear all
  - `b` switch to browse view

- Browse view
  - Search bar: type fuzzy query
  - `Tab` switch between search and result list
  - Results: `Up/Down` navigate
  - `Enter` or `Space` add highlighted package to queue
  - `d` remove highlighted package from queue
  - `/` focus search, `Esc` clear/back

- Installing view
  - `q` or `c` requests safe abort

- Done view
  - `r` back to queue
  - `Enter` quit

## Logs

Install logs are written to:

- `~/.cache/arch-package-tui/install-<timestamp>.log`
