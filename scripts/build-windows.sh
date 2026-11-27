#!/usr/bin/env bash
# Cross-compile Fetchly for Windows (x64) from macOS or Linux and package it.
# Produces in target/windows/:
#   Fetchly-Setup-<version>.exe   installer (per-user, no admin needed)
#   Fetchly-<version>-portable.zip  just the exe, runs without installing
#
# One-time setup on macOS:  brew install mingw-w64 makensis
#                           rustup target add x86_64-pc-windows-gnu
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

TARGET=x86_64-pc-windows-gnu
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT=target/windows

for tool in x86_64-w64-mingw32-gcc makensis; do
  command -v "$tool" >/dev/null || { echo "Missing $tool — run: brew install mingw-w64 makensis"; exit 1; }
done
rustup target add "$TARGET" >/dev/null

cargo build --release -p fetchly --target "$TARGET"
EXE="$PWD/target/$TARGET/release/fetchly.exe"
# The GNU build loads WebView2 through Microsoft's redistributable loader DLL.
LOADER=$(find "target/$TARGET/release/build" -path "*webview2-com-sys*/out/x64/WebView2Loader.dll" | head -1)
[ -n "$LOADER" ] || { echo "WebView2Loader.dll not found in the build output"; exit 1; }
LOADER="$PWD/$LOADER"

mkdir -p "$OUT"
rm -f "$OUT"/Fetchly-*

# Installer
(cd assets/windows && makensis -V2 \
  -DVERSION="$VERSION" \
  -DEXE="$EXE" \
  -DLOADER="$LOADER" \
  -DOUTFILE="$PWD/../../$OUT/Fetchly-Setup-$VERSION.exe" \
  installer.nsi)

# Portable zip
STAGE="$OUT/Fetchly"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp "$EXE" "$STAGE/Fetchly.exe"
cp "$LOADER" "$STAGE/WebView2Loader.dll"
(cd "$OUT" && zip -qr "Fetchly-$VERSION-portable.zip" Fetchly)
rm -rf "$STAGE"

echo "Built $OUT/Fetchly-Setup-$VERSION.exe"
echo "Built $OUT/Fetchly-$VERSION-portable.zip"
