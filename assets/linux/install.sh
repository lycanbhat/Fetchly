#!/usr/bin/env sh
# Installs Fetchly for the current user (no root needed).
# Usage: ./install.sh            -> installs to ~/.local
#        ./install.sh --uninstall
set -e
cd "$(dirname "$0")"
BIN="$HOME/.local/bin"; APPS="$HOME/.local/share/applications"; ICONS="$HOME/.local/share/icons/hicolor/512x512/apps"
if [ "$1" = "--uninstall" ]; then
  rm -f "$BIN/fetchly" "$APPS/fetchly.desktop" "$ICONS/fetchly.png"
  echo "Fetchly removed. Your data is kept in ~/.local/share/fetchly."
  exit 0
fi
install -Dm755 fetchly "$BIN/fetchly"
install -Dm644 fetchly.png "$ICONS/fetchly.png"
install -Dm644 fetchly.desktop "$APPS/fetchly.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
echo "Fetchly installed. Launch it from your app menu, or run: $BIN/fetchly"
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Tip: add $BIN to your PATH to run 'fetchly' from a terminal." ;; esac
