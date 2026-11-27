#!/usr/bin/env bash
# Build Fetchly.app and a shareable Fetchly-<version>.dmg for macOS.
#   ./scripts/bundle-macos.sh            -> for this Mac's CPU
#   ./scripts/bundle-macos.sh universal  -> runs on Apple Silicon and Intel (use this for sharing)
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT=target/macos
APP="$OUT/Fetchly.app"

if [ "${1:-}" = "universal" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release -p fetchly --target aarch64-apple-darwin
  cargo build --release -p fetchly --target x86_64-apple-darwin
  BIN="$OUT/fetchly-universal"
  mkdir -p "$OUT"
  lipo -create -output "$BIN" \
    target/aarch64-apple-darwin/release/fetchly \
    target/x86_64-apple-darwin/release/fetchly
else
  cargo build --release -p fetchly
  BIN=target/release/fetchly
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Fetchly"
cp assets/macos/Fetchly.icns "$APP/Contents/Resources/Fetchly.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Fetchly</string>
  <key>CFBundleDisplayName</key><string>Fetchly</string>
  <key>CFBundleIdentifier</key><string>dev.fetchly.app</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>Fetchly</string>
  <key>CFBundleIconFile</key><string>Fetchly</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

# Ad-hoc signature so macOS will launch it locally.
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true

echo "Built $APP"

# Disk image: the app plus an Applications shortcut to drag it onto.
DMG="$OUT/Fetchly-${VERSION}.dmg"
STAGE="$OUT/dmg"
rm -rf "$STAGE" "$DMG"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Fetchly ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "Built $DMG"
