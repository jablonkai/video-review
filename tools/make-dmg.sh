#!/bin/sh
# Builds target/release/Video Review-<version>.dmg — a drag-to-install macOS disk image.
# Layout inside the dmg: Video Review.app, an Applications alias, and a ReadMe.
#
# Notes:
#  * The app is NOT code-signed (no Apple Developer ID). On first launch the user
#    must right-click the app and choose Open (Gatekeeper).
#  * The app is arm64-only and links Homebrew's libmpv from /opt/homebrew by absolute
#    path (not bundled), so it won't start without brew install mpv; ffmpeg is needed
#    for thumbnails and scene detection (documented in the dmg ReadMe and README.md).
set -eu
cd "$(dirname "$0")/.."

APP_NAME="Video Review"
VERSION=$(cargo pkgid | sed 's/.*[#@]//')
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

cat > "$STAGING/ReadMe - Video Review.txt" <<README
Video Review — Mac installer / macOS telepítő ($VERSION)
=====================================================

Telepítés / Install:
1. Duple kattints a Video Review-$VERSION.dmg fájlra / Double-click the .dmg.
2. Húsd a "Video Review.app" ikont az Applications aliasra /
   Drag "Video Review.app" onto the Applications alias.

Első indítás / First launch:
Az alkalmazás nincs aláírva Apple fejlesztői ID-vel, ezért a Mac blokkolhatja.
Ebben az esetben a Finderben jobb gombbal kattints az alkalmazásra → "Megnyitás".
The app is not code-signed, so macOS may block it. In Finder, right-click the app
→ Open, then open it again.

Követelmények / Requirements:
Apple Silicon (M1 vagy újabb) Mac és Homebrew az alapértelmezett /opt/homebrew
helyen / An Apple Silicon (M1 or later) Mac and Homebrew in its default
/opt/homebrew prefix. Telepítsd indítás előtt / Install before running:
    brew install mpv ffmpeg
Az mpv nélkül (vagy Intel Macen) az alkalmazás el sem indul / Without mpv (or on
an Intel Mac) the app does not start at all.
Az ffmpeg nélkül a filmstrip-képek és a vágásfelismerés nem működnek, ezt az
alkalmazás jelzi / Without ffmpeg, thumbnails and scene detection are disabled;
the app shows a warning.

Ismerd többet / Read more: README.md a forráskönyvtárban /
Read more: README.md in the source repository.
README

echo "==> creating disk image"
# Read-write working image, so Finder can write .DS_Store into it.
hdiutil create -srcfolder "$STAGING" -volname "$VOLUME" -ov -format UDRW -fs HFS+ "$TMP_DMG" >/dev/null

echo "==> arranging dmg window"
# Take the mount point from hdiutil's output: if another "Video Review" volume is
# already mounted, this one lands at "/Volumes/Video Review 1".
ATTACH_OUT=$(hdiutil attach -readwrite -nobrowse -noverify -noautoopen "$TMP_DMG")
DEVICE=$(printf '%s\n' "$ATTACH_OUT" | awk 'NR == 1 { print $1 }')
MOUNT=$(printf '%s\n' "$ATTACH_OUT" | sed -n 's|^.*[[:space:]]\(/Volumes/.*\)$|\1|p' | head -n 1)
trap 'hdiutil detach "$DEVICE" >/dev/null 2>&1 || true' EXIT
[ -n "$MOUNT" ] && [ -d "$MOUNT" ] || { echo "error: volume did not mount" >&2; exit 1; }

# Have Finder store the window layout and icon positions in .DS_Store. The volume
# is addressed by path, not by name, so a same-named volume is never touched.
osascript >/dev/null <<EOF
tell application "Finder"
    set d to item (POSIX file "$MOUNT" as alias)
    open d
    tell container window of d
        set current view to icon view
        set toolbar visible to false
        set statusbar visible to false
        set the bounds to {420, 120, 940, 460}
    end tell
    set arrangement of icon view options of container window of d to not arranged
    set position of item "$APP" of d to {160, 190}
    set position of item "Applications" of d to {480, 190}
    update d without registering applications
    delay 2
    close container window of d
end tell
EOF

# Finder writes .DS_Store asynchronously; wait for it before detaching.
for i in $(seq 1 20); do
    if [ -f "$MOUNT/.DS_Store" ]; then break; fi
    sleep 0.5
done
[ -f "$MOUNT/.DS_Store" ] || echo "warning: Finder wrote no .DS_Store; the dmg window will use the default layout" >&2
sync

hdiutil detach "$DEVICE" >/dev/null
trap - EXIT

echo "==> compressing"
hdiutil convert "$TMP_DMG" -format UDZO -imagekey zlib-level=9 -o "$DMG" >/dev/null
rm -f "$TMP_DMG"
rm -rf "$STAGING"

echo "==> done: $DMG"
ls -la "$DMG"
