# Qur on Linux

Branch `linux-port`. Qur builds and runs on Linux (Wayland and X11) with macOS behaviour unchanged: every Linux path is behind `cfg(target_os = ...)`. Tested on Ubuntu 26.04 and CachyOS (Arch), Hyprland (Wayland), a GTX 1650 / Intel laptop with 7 GB RAM.

## Build and install

```sh
# Arch / CachyOS
sudo pacman -S --needed rustup base-devel cmake clang mold libxkbcommon libxkbcommon-x11 wayland vulkan-icd-loader \
  fontconfig freetype2 openssl libxcb alsa-lib zstd webkitgtk-6.0 python-gobject wl-clipboard
# Ubuntu / Debian
sudo apt-get install build-essential pkg-config cmake clang mold libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev libssl-dev libx11-xcb-dev libxcb1-dev \
  libzstd-dev libasound2-dev \
  gir1.2-webkit-6.0 python3-gi wl-clipboard xclip
curl https://sh.rustup.rs -sSf | sh            # stable Rust
CARGO_BUILD_JOBS=2 RUSTFLAGS="-C link-arg=-fuse-ld=mold" scripts/bundle-linux.sh --install
```

`bundle-linux.sh` builds `target/release/qur`, writes `qur.desktop` and the icon to `target/release/linux/`, packs them into `target/release/qur-<version>-linux-<arch>.tar.gz` (the Linux release download, like the macOS .dmg), and with `--install` puts them in `~/.local/bin`, `~/.local/share/applications` and `~/.local/share/icons/hicolor/512x512/apps`, so Qur shows up in GNOME's and Hyprland's launchers (wofi, fuzzel, ...). The window's app id is `qur` (matches `StartupWMClass`). On a 7 GB machine use 2 jobs; a release build takes about 10 minutes from scratch.

## What changed

| Area | macOS | Linux |
|---|---|---|
| GPUI | AppKit | `gpui`/`gpui_platform` `wayland` + `x11` features (target-specific deps in `qur_app/Cargo.toml`) |
| Window | transparent title bar, system traffic lights hidden | client-side decorations (`WindowDecorations::Client`); Qur's own bar and lights work as before; drag the outer 6 px to resize (off when maximized or tiled) |
| Keys | as written in `commands.rs` | translated at bind time (`linux_keys`): Cmd -> Ctrl; Mac Ctrl shortcuts -> Alt (Ctrl+T switcher is Alt+T, Ctrl+1..9 sessions are Alt+1..9, Go to Line Alt+G); text keys follow Linux (Home/End, Ctrl+Home/End, Ctrl+arrows and Ctrl+Backspace by word); Ctrl+Cmd+F -> F11; in a terminal, Cmd keys become Ctrl+Shift (copy Ctrl+Shift+C, paste Ctrl+Shift+V) so Ctrl+C, Ctrl+B etc. reach the shell. Shortcuts are shown as `Ctrl+Shift+P`. `keymap.toml` is still written with Mac names and translated the same way. |
| Settings, logs | `~/Library/Application Support/Qur`, `~/Library/Logs/Qur` | `$XDG_DATA_HOME/qur` (`~/.local/share/qur`), `$XDG_STATE_HOME/qur` (`~/.local/state/qur`) |
| Browser | embedded WKWebView with Inspect | WebKitGTK 6 in a helper process (`qur_browser/src/webview.py`, PyGObject) driven over stdin/stdout by `linux.rs`, same API and the same `inspect.js`: Inspect, picks, text selection, element snapshots, edit commands, zoom, Back/Forward. On Hyprland its window floats exactly over the Browser pane and follows Qur's window and workspace; on other desktops it's a window beside Qur. Without WebKitGTK, pages open in the default browser. |
| Alerts | notification, Dock badge, chime (NSSound) | notifications through GPUI (freedesktop); badge as the Unity LauncherEntry D-Bus signal (KDE, Dash to Dock, Plank show the count); chime played with `pw-play`, `paplay` or `aplay` |
| Open / Reveal | `/usr/bin/open`, `open -R` | `files::system_open`: `xdg-open`; reveal uses the FileManager1 D-Bus call (Nautilus etc.), else opens the folder |
| Keep Awake | `caffeinate -dims -w <pid>` | `systemd-inhibit --what=idle:sleep ... tail --pid=<pid>` |
| Processes (memory readout, terminal titles, a shell's folder, session kill, doctor) | libproc / sysctl | `/proc` (`mem.rs`: `all_pids`, `stat_fields`, footprint = VmRSS + VmSwap; `terminal::cwd_of` reads `/proc/<pid>/cwd`) |
| Paste images / copied files into a terminal | NSPasteboard | `wl-paste` (Wayland) or `xclip` (X11): PNG, JPEG, TIFF |
| Cmd-click (terminal links, Git and Changes rows), Cmd+arrows in large/hex views | Cmd | Ctrl (GPUI `secondary()`) |
| Hide (Cmd+H) | hides the app | minimizes the window |
| Free memory after big decodes | `malloc_zone_pressure_relief` | glibc `malloc_trim(0)` |
| `qur doctor`, crash reports | macOS version | `/etc/os-release` |
| File watcher | FSEvents | inotify (notify's Linux backend, no change needed) |

## Tested

- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --release --workspace --no-fail-fast`: all pass (qur_app 119, qur_browser incl. the helper protocol tests). `qur_syntax::bench_large_ts` gets twice the Apple-silicon time budget off macOS. Needs `python3 spikes/fixtures/gen.py` first.
- `cargo build --release -p qur_app`: about 50 MB (macOS: 25 MB; the Linux GPUI backend pulls in Wayland, X11, Vulkan and zbus).
- `scripts/handcheck.sh` (the scripted UI checks; the driver now presses the Linux keys a user would): 111 of 125 pass on Hyprland with Qur floated at 1400x1000 (`hyprctl keyword windowrule "float on, size 1400 1000, match:class ^qur$"`; a small tiled window puts the clicks off-window) and `SHELL=/bin/bash` (a zsh setup wizard eats the typed commands). The rest are not Linux bugs:
  - `empty_window`, `empty_recent` (8): stale since v0.1.8, which opens a shell when Qur starts without a path; they fail on macOS too.
  - `cleanup` (3-5): the test backdates worktrees with `touch -t`, which on macOS also moves a file's birth time; Linux can't, so the worktrees stay "new" and aren't preselected (the grace period working as meant).
- Browser, with the fake ACP agent (`target/release/qur-fake-agent` as an `acp` agent, `QUR_DEBUG_BROWSER_URL`, `QUR_DEBUG_PICK`): the page loads over the pane, Inspect outlines the element, the pick and its snapshot land in the prompt; the approval notification and badge fire.
- Clipboard image read (`wl-paste --type image/png`) and the badge signal checked by hand.
- `qur doctor` runs (Linux line, git, settings, Claude sign-in).

## Stubbed or missing

- **Browser placement off Hyprland:** Wayland doesn't let a client place its windows, and GPUI has no GTK widget tree to host WebKitGTK in, so on GNOME/KDE the page is a separate window beside Qur (Inspect and picks still work). On Hyprland a float rule keeps it over the pane, but another window you raise on the same workspace can't cover it.
- **Menu bar:** GPUI has no global menu on Linux; everything is in the Command Palette (Ctrl+Shift+P) and shortcuts.
- **Hide Others / Show All:** no meaning on Linux compositors (actions exist, do nothing visible).
- **Window gaps / transparency:** use GPUI's Linux background modes; blur depends on the compositor (Hyprland blurs, GNOME doesn't).

## Known gaps / to check by hand

- GNOME (mutter) and KDE sessions not tested on this laptop (Hyprland only). X11 not tested.
- Structured agents need `claude-agent-acp` on PATH (doctor reports it missing here); Agent Terminals work with `claude` alone.
- `scripts/budget.sh`, `memcheck.sh` and `bundle.sh` are macOS-only (Swift, `footprint`, `screencapture`, `.app`/.dmg); `handcheck.sh` runs on Linux.
- Key translation is rule-based; a few Mac combos may land on keys a Linux desktop already uses (e.g. Ctrl+Alt+arrows in some WMs). Rebind in `keymap.toml`.
- Release binary is 50 MB, over the 40 MB macOS budget.
