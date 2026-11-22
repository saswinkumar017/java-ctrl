#!/usr/bin/env bash
# Java Control Centre — Omarchy / Arch Linux installer.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/java-control-centre/java-control-centre/main/installers/omarchy/install.sh | bash
#   JCC_VERSION=0.1.0 ./install.sh
#   JCC_REPO=your-org/java-control-centre ./install.sh
#
# Installs the prebuilt AppImage from GitHub Releases. No Rust or Node.js needed.

set -euo pipefail

REPO="${JCC_REPO:-java-control-centre/java-control-centre}"
VERSION="${JCC_VERSION:-}"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_DIR/java-control-centre"
APP_NAME="java-control-centre"

info() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[!]\033[0m %s\n' "$*" >&2; }
fail() { printf '\033[1;31m[x]\033[0m %s\n' "$*" >&2; exit 1; }

command -v pacman >/dev/null 2>&1 || fail "This installer is for Arch Linux / Omarchy (pacman not found)."
command -v curl >/dev/null 2>&1 || fail "curl is required."

info "Installing runtime dependencies"
DEPS=(webkit2gtk-4.1 gtk3 libayatana-appindicator fuse2)
if command -v jcmd >/dev/null 2>&1; then
  info "Found jcmd — skipping JDK installation"
else
  info "No jcmd found — adding JDK 21"
  DEPS+=(jdk21-openjdk)
fi
if [[ $EUID -eq 0 ]]; then
  pacman -Syu --needed --noconfirm "${DEPS[@]}"
else
  sudo pacman -Syu --needed --noconfirm "${DEPS[@]}"
fi

if [[ -z "$VERSION" ]]; then
  info "Looking up the latest release"
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' | cut -d'"' -f4)
  [[ -n "$VERSION" ]] || fail "Could not determine the latest release. Set JCC_VERSION."
fi
VERSION="${VERSION#v}"
info "Installing Java Control Centre $VERSION"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

ASSET="${APP_NAME}_${VERSION}_amd64.AppImage"
BASE="https://github.com/$REPO/releases/download"
if ! curl -fL --retry 3 -o "$TMP_DIR/$APP_NAME.AppImage" "$BASE/v$VERSION/$ASSET"; then
  warn "Tagged release asset not found, trying without the v prefix"
  curl -fL --retry 3 -o "$TMP_DIR/$APP_NAME.AppImage" "$BASE/$VERSION/$ASSET" \
    || fail "Could not download $ASSET from $REPO releases."
fi
chmod +x "$TMP_DIR/$APP_NAME.AppImage"

info "Installing to $APP_DIR"
mkdir -p "$APP_DIR" "$BIN_DIR" "$DATA_DIR/applications" \
  "$DATA_DIR/icons/hicolor/128x128/apps"
install -m 755 "$TMP_DIR/$APP_NAME.AppImage" "$APP_DIR/$APP_NAME.AppImage"
ln -sfn "$APP_DIR/$APP_NAME.AppImage" "$BIN_DIR/$APP_NAME"

if curl -fsSL -o "$DATA_DIR/icons/hicolor/128x128/apps/$APP_NAME.png" \
  "https://raw.githubusercontent.com/$REPO/main/src-tauri/icons/128x128.png"; then
  ICON="$APP_NAME"
else
  ICON="$APP_DIR/$APP_NAME.AppImage"
fi

cat > "$DATA_DIR/applications/$APP_NAME.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Java Control Centre
GenericName=JVM Control Centre
Comment=Discover JDKs, monitor JVMs, analyse heap dumps and Spring Boot Actuator
Exec=$APP_DIR/$APP_NAME.AppImage
Icon=$ICON
Terminal=false
Categories=Development;IDE;Utility;
Keywords=java;jvm;jdk;heap;gc;spring;
EOF
update-desktop-database "$DATA_DIR/applications" 2>/dev/null || true

info "Done. Launch with: $BIN_DIR/$APP_NAME"
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
  warn "$BIN_DIR is not on your PATH. Add: export PATH=\"$BIN_DIR:\$PATH\""
fi
