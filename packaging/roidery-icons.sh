#!/usr/bin/env bash
# Roidery: regenerate every app icon from assets/roidery-icon/roidery.svg (the canonical artwork).
#
# Needs: resvg (brew install resvg / cargo install resvg). The .icns comes from iconutil on macOS,
# else from `cargo xtask icns`. The outputs are committed, so building and packaging never need
# these tools.
#
#   packaging/roidery-icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/roidery-icon"
SVG="$DIR/roidery.svg"
ID="app.roidery.Roidery"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The SVG is a full-bleed 512 tile, used as-is on Windows and Linux. macOS icons follow Apple's grid:
# the tile is 824/1024 of the canvas with a transparent margin, so widen the viewBox around it.
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62.136 -62.136 636.272 636.272"/' "$SVG" >"$MAC"
grep -q 'viewBox="-62.136' "$MAC" || { echo "error: $SVG has no 512 viewBox" >&2; exit 1; }
# Phones round the corners of a home-screen icon themselves, so it is square and opaque.
SQUARE="$TMP/square.svg"
sed 's|<rect width="512" height="512" rx="112"/>|<rect width="512" height="512"/>|' "$SVG" >"$SQUARE"

render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }
xtask() { (cd "$ROOT" && cargo run -q -p xtask -- "$@"); }

render "$SVG" 1024 "$DIR/roidery-1024.png"
render "$MAC" 512 "$DIR/roidery-macos-512.png"
render "$SQUARE" 180 "$DIR/apple-touch-icon.png"

for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$SVG" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

for s in 16 20 24 32 40 48 64 128 256; do render "$SVG" "$s" "$TMP/ico-$s.png"; done
xtask ico "$DIR/roidery.ico" "$TMP"/ico-{16,20,24,32,40,48,64,128,256}.png
xtask ico "$DIR/favicon.ico" "$TMP"/ico-{16,32,48}.png

for s in 16 32 64 128 256 512 1024; do render "$MAC" "$s" "$TMP/mac-$s.png"; done
if command -v iconutil >/dev/null; then
  SET="$TMP/roidery.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    cp "$TMP/mac-$s.png" "$SET/icon_${s}x${s}.png"
    cp "$TMP/mac-$((s * 2)).png" "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/roidery.icns" "$SET"
else
  xtask icns "$DIR/roidery.icns" "$TMP"/mac-{16,32,64,128,256,512,1024}.png
fi
echo "icons written to $DIR"
