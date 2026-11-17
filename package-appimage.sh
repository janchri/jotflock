#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo >/dev/null || { echo 'Rust/cargo is required' >&2; exit 1; }
linuxdeploy_bin="${LINUXDEPLOY:-linuxdeploy}"
command -v "$linuxdeploy_bin" >/dev/null || { echo 'linuxdeploy with its AppImage output plugin is required' >&2; exit 1; }
arch=$(uname -m)
case "$arch" in x86_64|aarch64) ;; *) echo "Unsupported architecture: $arch" >&2; exit 1;; esac
cargo build --release -p jotflock
bundle="$PWD/target/jotflock.AppDir"
mkdir -p "$bundle/usr/bin"
cp target/release/jotflock "$bundle/usr/bin/"
cat > "$bundle/jotflock.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Jotflock
Comment=Native collaborative whiteboard
Exec=jotflock %u
Icon=jotflock
Categories=Office;
Terminal=false
MimeType=x-scheme-handler/jotflock;
DESKTOP
cp assets/jotflock.png "$bundle/jotflock.png"
install -Dm644 assets/fonts/OFL.txt "$bundle/usr/share/licenses/jotflock/Roboto-OFL.txt"
# Bundle linked libraries rather than shipping an executable-only AppDir.
ARCH="$arch" LDAI_OUTPUT="$PWD/jotflock-$arch.AppImage" APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy_bin" \
  --appdir "$bundle" --executable "$bundle/usr/bin/jotflock" \
  --desktop-file "$bundle/jotflock.desktop" --icon-file "$bundle/jotflock.png" --output appimage
test -s "$PWD/jotflock-$arch.AppImage"
printf 'AppImage created: %s\n' "$PWD/jotflock-$arch.AppImage"
