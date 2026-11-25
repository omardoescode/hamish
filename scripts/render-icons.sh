#!/bin/sh
# Renders the hicolor PNG icons and the macOS .icns from packaging/icons/hamish.svg.
# Needs rsvg-convert and uv (uvx runs icnsutil).
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
svg="$root/packaging/icons/hamish.svg"

for n in 16 24 32 48 64 128 256 512; do
  dir="$root/packaging/icons/hicolor/${n}x${n}/apps"
  mkdir -p "$dir"
  rsvg-convert -w "$n" -h "$n" "$svg" -o "$dir/hamish.png"
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for n in 16 32 128 256 512; do
  rsvg-convert -w "$n" -h "$n" "$svg" -o "$work/${n}.png"
  rsvg-convert -w "$((n * 2))" -h "$((n * 2))" "$svg" -o "$work/${n}@2x.png"
done
uvx --from icnsutil icnsutil compose -f "$root/packaging/macos/Hamish.icns" \
  "$work/16.png" "$work/16@2x.png" "$work/32.png" "$work/32@2x.png" \
  "$work/128.png" "$work/128@2x.png" "$work/256.png" "$work/256@2x.png" \
  "$work/512.png" "$work/512@2x.png"
