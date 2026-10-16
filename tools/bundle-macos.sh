#!/bin/sh
# Builds target/release/Video Review.app (uses Homebrew's libmpv at runtime).
set -e
cd "$(dirname "$0")/.."
cargo build --release --quiet
APP="target/release/Video Review.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/video-review "$APP/Contents/MacOS/video-review"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Video Review</string>
  <key>CFBundleDisplayName</key><string>Video Review</string>
  <key>CFBundleIdentifier</key><string>hu.jablonkai.video-review</string>
  <key>CFBundleExecutable</key><string>video-review</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "$APP"
