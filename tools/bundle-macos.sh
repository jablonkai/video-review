#!/bin/sh
# Builds target/release/Video Review.app (uses Homebrew's libmpv at runtime).
set -e
cd "$(dirname "$0")/.."
cargo build --release --quiet
VERSION=$(cargo pkgid | sed 's/.*[#@]//')
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
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key><array>
    <dict>
      <key>CFBundleTypeName</key><string>Video</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key><array><string>public.movie</string></array>
    </dict>
    <dict>
      <!-- model::VIDEO_EXTENSIONS, for types macOS doesn't know as movies (mkv, webm, mxf). -->
      <key>CFBundleTypeName</key><string>Video file</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>CFBundleTypeExtensions</key><array>
        <string>mp4</string><string>m4v</string><string>mov</string><string>webm</string>
        <string>mkv</string><string>mxf</string><string>avi</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Folder</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>None</string>
      <key>LSItemContentTypes</key><array><string>public.folder</string></array>
    </dict>
  </array>
</dict></plist>
PLIST
echo "$APP"
