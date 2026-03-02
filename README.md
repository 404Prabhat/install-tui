# arch-package-tui

Keyboard-first Arch package commander with a unified tri-pane TUI and autonomous fallback install engine.

## Highlights

- Single always-on layout:
  - command bar
  - package table (fuzzy filter)
  - detail pane
  - live activity feed
  - persistent queue bar
- Modes: `NORMAL`, `FILTER` (`/`), `COMMAND` (`:`)
- Backend abstraction + auto-detection:
  - `pacman`, `yay`, `paru`, `aura`, `trizen`
- Reorder backend priority at runtime (`Alt+Up/Alt+Down`)
- SQLite package cache:
  - `~/.cache/arch-package-tui/pkgdb.sqlite`
  - instant cache load + background sync
- Fuzzy matching via `nucleo-matcher`
- Queue actions:
  - install mode
  - removal mode (`r`)
- Install preview overlay before execution
- Live activity stream with timestamps and status markers
- Headless mode for scripting (`--no-tui`)

## Arch-only behavior

The app validates Arch Linux (`/etc/arch-release`), non-root execution, and internet reachability before install operations.

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run
```

## Install globally (invoke from any directory)

```bash
mkdir -p ~/.local/bin
cp target/release/arch-package-tui ~/.local/bin/
chmod +x ~/.local/bin/arch-package-tui
```

Ensure `~/.local/bin` is in your `PATH`, then run:

```bash
arch-package-tui
```

## Headless examples

```bash
arch-package-tui --install btop fastfetch ripgrep --no-tui
arch-package-tui --remove firefox --no-tui
arch-package-tui --load my-setup --no-tui
```

## Core keybinds (NORMAL)

- `j`/`k` or arrow keys: move
- `g` / `G`: top / bottom
- `Enter`: toggle queue on selected package
- `i`: open install preview and start install
- `u`: upgradable packages view
- `U`: full system upgrade
- `r`: toggle removal mode
- `s`: cycle sort mode
- `S`: sync package database
- `d`: remove highlighted from queue
- `x`: clear queue
- `t`: toggle dry-run
- `Tab`: focus cycle list/detail/feed
- `:`: command mode
- `/`: filter mode
- `?`: keybind overlay
- `q`: quit

## Command mode examples

- `:search neovim`
- `:install fastfetch btop nvtop`
- `:remove firefox`
- `:info neovim`
- `:sync`
- `:upgrade`
- `:save gaming`
- `:load gaming`
- `:orphans`
- `:history`

## Logs

Install logs are written under:

- `~/.cache/arch-package-tui/install-<timestamp>.log`
