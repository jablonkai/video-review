#!/bin/sh
# Builds target/release/Video Review-<version>.dmg — a drag-to-install macOS disk image.
# Layout inside the dmg: Video Review.app, an Applications alias, and a ReadMe.
#
# Notes:
#  * The app is NOT code-signed (no Apple Developer ID). On first launch the user
#    must right-click the app and choose Open (Gatekeeper).
#  * The app needs Homebrew's mpv and ffmpeg at runtime: brew install mpv ffmpeg
#    (documented in the ReadMe inside the dmg and in README.md).
set -eu
cd "$(dirname "$0")/.."

APP_NAME="Video Review"
VERSION="0.1.0"
APP="$APP_NAME.app"
DMG="target/release/${APP_NAME}-${VERSION}.dmg"
TMP_DMG="target/release/${APP_NAME}-${VERSION}-unsigned.dmg"
STAGING="target/release/dmg-staging"
VOLUME="$APP_NAME"

echo "==> bundling .app"
./tools/bundle-macos.sh >/dev/null

echo "==> preparing dmg staging"
rm -rf "$STAGING" "$DMG" "$TMP_DMG"
mkdir -p "$STAGING"
cp -R "target/release/$APP" "$STAGING/$APP"
ln -s /Applications "$STAGING/Applications"

cat > "$STAGING/ReadMe - Video Review.txt" <<'README'
Video Review — Mac installer / macOS telepítő (0.1.0)
=====================================================

Telepítés / Install:
1. Duple kattints a Video Review-0.1.0.dmg fájlra / Double-click the .dmg.
2. Húsd a "Video Review.app" ikont az Applications aliasra /
   Drag "Video Review.app" onto the Applications alias.

Első indítás / First launch:
Az alkalmazás nincs aláírva Apple fejlesztői ID-vel, ezért a Mac blokkolhatja.
Ebben az esetben a Finderben jobb gombbal kattints az alkalmazásra → "Megnyitás".
The app is not code-signed, so macOS may block it. In Finder, right-click the app
→ Open, then open it again.

Kötelező könyvtárak / Required libraries:
A program a Homebrew-beli mpv és ffmpeg-hez nyúl / The app needs Homebrew's mpv
and ffmpeg. Install them before running:
    brew install mpv ffmpeg
Nélkülük a lejátszás, a filmstrip-képek és a scenes felismerés nem működnek /
Without them playback, thumbnails and scene detection are disabled.

Ismerd többet / Read more: README.md a forráskönyvtárban /
Read more: README.md in the source repository.
README

echo "==> creating disk image"
hdiutil create -srcfolder "$STAGING" -volname "$VOLUME" -ov -format UDZO -fs HFS+ "$TMP_DMG" >/dev/null

echo "==> arranging dmg window"
hdiutil attach -noautoopen "$TMP_DMG" >/dev/null
trap 'hdiutil detach "/Volumes/$VOLUME" >/dev/null 2>&1 || true' EXIT

# Wait until the volume mounts, then have Finder store icon positions in .DS_Store.
for i in $(seq 1 30); do
    if [ -d "/Volumes/$VOLUME" ]; then break; fi
    sleep 0.5
done
[ -d "/Volumes/$VOLUME" ] || { echo "error: volume did not mount" >&2; exit 1; }
sleep 1
osascript >/dev/null <<EOF
tell application "Finder"
    set d to disk "$VOLUME"
    open d
    tell container window of d
        set current view to icon view
        set toolbar visible to false
        set statusbar visible to false
        set the bounds to {420, 120, 940, 460}
        set position of item "$APP" to {160, 190}
        set position of item "Applications" to {480, 190}
    end tell
    delay 2
    close d
end tell
EOF

hdiutil detach "/Volumes/$VOLUME" >/dev/null
trap - EXIT

echo "==> compressing"
hdiutil convert "$TMP_DMG" -format UDZO -o "$DMG" >/dev/null
rm -f "$TMP_DMG"
rm -rf "$STAGING"

echo "==> done: $DMG"
ls -la "$DMG"
