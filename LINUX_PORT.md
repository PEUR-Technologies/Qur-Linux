# Qur on Linux

Branch `linux-port`. Qur builds and runs on Linux (Wayland and X11) with macOS behaviour unchanged: every Linux path is behind `cfg(target_os = ...)`. Tested on Ubuntu 26.04, Hyprland (Wayland), a GTX 1650 / Intel laptop with 7 GB RAM.

## Build and install

```sh
sudo apt-get install build-essential pkg-config cmake clang mold libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev libssl-dev libx11-xcb-dev libxcb1-dev \
  libzstd-dev libasound2-dev
curl https://sh.rustup.rs -sSf | sh            # stable Rust
CARGO_BUILD_JOBS=2 RUSTFLAGS="-C link-arg=-fuse-ld=mold" scripts/bundle-linux.sh --install
```

`bundle-linux.sh` builds `target/release/qur`, writes `qur.desktop` and the icon to `target/release/linux/`, and with `--install` puts them in `~/.local/bin`, `~/.local/share/applications` and `~/.local/share/icons/hicolor/512x512/apps`, so Qur shows up in GNOME's and Hyprland's launchers (wofi, fuzzel, ...). The window's app id is `qur` (matches `StartupWMClass`). On a 7 GB machine use 2 jobs; a release build takes about 10 minutes from scratch.

## What changed

| Area | macOS | Linux |
|---|---|---|
| GPUI | AppKit | `gpui`/`gpui_platform` `wayland` + `x11` features (target-specific deps in `qur_app/Cargo.toml`) |
| Window | transparent title bar, system traffic lights hidden | client-side decorations (`WindowDecorations::Client`); Qur's own bar and lights work as before; drag the outer 6 px to resize (off when maximized or tiled) |
| Keys | as written in `commands.rs` | translated at bind time (`linux_keys`): Cmd -> Ctrl; Mac Ctrl shortcuts -> Alt (Ctrl+T switcher is Alt+T, Ctrl+1..9 sessions are Alt+1..9, Go to Line Alt+G); text keys follow Linux (Home/End, Ctrl+Home/End, Ctrl+arrows and Ctrl+Backspace by word); Ctrl+Cmd+F -> F11; in a terminal, Cmd keys become Ctrl+Shift (copy Ctrl+Shift+C, paste Ctrl+Shift+V) so Ctrl+C, Ctrl+B etc. reach the shell. Shortcuts are shown as `Ctrl+Shift+P`. `keymap.toml` is still written with Mac names and translated the same way. |
| Settings, logs | `~/Library/Application Support/Qur`, `~/Library/Logs/Qur` | `$XDG_DATA_HOME/qur` (`~/.local/share/qur`), `$XDG_STATE_HOME/qur` (`~/.local/state/qur`) |
| Browser | embedded WKWebView with Inspect | `qur_browser/src/linux.rs`: same API, pages open in the default browser (`xdg-open`); Back/Forward keep a history; Inspect, page snapshots and in-page edit commands report "not available" |
| Alerts | notification, Dock badge, chime (NSSound) | notifications through GPUI (freedesktop); no badge; chime played with `pw-play`, `paplay` or `aplay` |
| Open / Reveal | `/usr/bin/open`, `open -R` | `files::system_open`: `xdg-open`; reveal uses the FileManager1 D-Bus call (Nautilus etc.), else opens the folder |
| Keep Awake | `caffeinate -dims -w <pid>` | `systemd-inhibit --what=idle:sleep ... tail --pid=<pid>` |
| Processes (memory readout, terminal titles, session kill, doctor) | libproc / sysctl | `/proc` (`mem.rs`: `all_pids`, `stat_fields`, footprint = VmRSS + VmSwap) |
| Free memory after big decodes | `malloc_zone_pressure_relief` | glibc `malloc_trim(0)` |
| `qur doctor`, crash reports | macOS version | `/etc/os-release` |
| File watcher | FSEvents | inotify (notify's Linux backend, no change needed) |

## Tested

- `cargo check --workspace --all-targets`: clean, no warnings.
- `cargo build --release -p qur_app`: 50 MB binary (macOS build is 25 MB; the Linux GPUI backend pulls in Wayland, X11, Vulkan/blade and zbus).
- `cargo test --release --workspace`: all pass (qur_app 118, incl. the new key-translation test) except `qur_syntax::bench_large_ts`, a speed budget tuned for Apple silicon (full parse 65 ms here vs 50 ms budget). Needs `python3 spikes/fixtures/gen.py` first.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `qur_watch`: inotify missed files written into a just-created folder; new folders are now listed on creation (Linux only), and the watch tests pass.
- Launched on Hyprland with a scratch `QUR_HOME`: window opens (class `qur`), text and icons render, sidebar, Source Control and file tree show; scripted actions (`New Terminal`, `Command Palette`, `Quit`) ran and it quit cleanly, nothing in the log.
- `qur doctor` runs (Linux line, git, settings, Claude sign-in).

## Stubbed or missing

- **Browser pane:** no embedded page. GPUI draws straight to Wayland/X11 with no GTK widget tree, so WebKitGTK can't be hosted inside the window without a new crate (wry/webkit2gtk) and an X11-only child window. Inspect needs that.
- **Dock badge:** none (no portable API; could use the Unity LauncherEntry D-Bus signal later).
- **Menu bar:** GPUI has no global menu on Linux; everything is in the Command Palette (Ctrl+Shift+P) and shortcuts.
- **Hide / Hide Others:** no meaning on Linux compositors (actions exist, do nothing visible).
- **Terminal paste of images and copied files:** the macOS pasteboard reader isn't ported (`files::pasteboard_image` returns None off macOS); text paste works.
- **Window gaps / transparency:** use GPUI's Linux background modes; blur depends on the compositor (Hyprland blurs, GNOME doesn't).

## Known gaps / to check by hand

- GNOME (mutter) session not tested on this laptop (GNOME was removed; Hyprland only). X11 not tested.
- Structured agents need `claude-agent-acp` on PATH (doctor reports it missing here); Agent Terminals work with `claude` alone.
- `scripts/handcheck.sh`, `budget.sh`, `memcheck.sh` and `bundle.sh` are macOS-only (Swift, `footprint`, `screencapture`).
- Key translation is rule-based; a few Mac combos may land on keys a Linux desktop already uses (e.g. Ctrl+Alt+arrows in some WMs). Rebind in `keymap.toml`.
- Release binary is 50 MB, over the 40 MB macOS budget.
