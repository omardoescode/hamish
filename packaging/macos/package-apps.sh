#!/bin/sh
# Turns the macOS release archives in target/distrib into zipped Hamish.app bundles in target/macos-app.
# cargo-dist runs this as an extra artifact in its global build job.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
distrib="$root/target/distrib"
out="$root/target/macos-app"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  archive="$distrib/hamish-$target.tar.xz"
  if [ ! -f "$archive" ]; then
    echo "package-apps.sh: missing $archive" >&2
    exit 1
  fi
  mkdir -p "$work/$target"
  tar -xJf "$archive" -C "$work/$target"
  sh "$root/packaging/macos/make-app.sh" "$work/$target/hamish-$target/hamish" "$work/$target" "$version"
  rm -f "$out/Hamish-$target.app.zip"
  (cd "$work/$target" && zip -q -r -X -y "$out/Hamish-$target.app.zip" Hamish.app)
done
