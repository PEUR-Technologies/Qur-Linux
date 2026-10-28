#!/usr/bin/env bash
# Build Qur for Linux: release binary, qur.desktop and icon.
# usage: scripts/bundle-linux.sh [--install]
#   -> target/release/qur, target/release/linux/{qur,qur.desktop,qur.png},
#      target/release/qur-<version>-linux-<arch>.tar.gz (the release download)
#   --install also copies them to ~/.local/bin, ~/.local/share/applications
#   and ~/.local/share/icons/hicolor/512x512/apps (shows in GNOME and Hyprland launchers).
# Low-memory machines: CARGO_BUILD_JOBS=2 (or 1) before running.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release -p qur_app

OUT=target/release/linux
mkdir -p "$OUT"
cp assets/brand/icons/icon-512.png "$OUT/qur.png"
desktop() {
  cat <<DESKTOP
[Desktop Entry]
Type=Application
Name=Qur
GenericName=Agentic Code Editor
Comment=See what your agents see
Exec=$1 %F
Icon=qur
Terminal=false
Categories=Development;IDE;TextEditor;
MimeType=inode/directory;
StartupWMClass=qur
Keywords=agent;editor;code;claude;
DESKTOP
}
desktop qur > "$OUT/qur.desktop"
cp target/release/qur "$OUT/qur"
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TAR="target/release/qur-$VER-linux-$(uname -m).tar.gz"
tar -czf "$TAR" -C "$OUT" qur qur.desktop qur.png
echo "built target/release/qur, $OUT/ and $TAR"

if [[ "${1:-}" == "--install" ]]; then
  BIN="$HOME/.local/bin"
  APPS="$HOME/.local/share/applications"
  ICONS="$HOME/.local/share/icons/hicolor/512x512/apps"
  mkdir -p "$BIN" "$APPS" "$ICONS"
  install -m 755 target/release/qur "$BIN/qur"
  install -m 644 "$OUT/qur.png" "$ICONS/qur.png"
  desktop "$BIN/qur" > "$APPS/qur.desktop"
  update-desktop-database "$APPS" 2>/dev/null || true
  gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
  echo "installed $BIN/qur and $APPS/qur.desktop"
fi
