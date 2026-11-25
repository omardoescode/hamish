#!/bin/sh
# Checks the desktop packaging with a stub binary: the Linux install script
# (install and uninstall under a temporary prefix, from the release archive layout)
# and the Hamish.app bundles made from macOS archives.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() {
  echo "packaging-test: $*" >&2
  exit 1
}

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$root/packaging/linux/hamish.desktop"
fi

archive="$work/hamish-x86_64-unknown-linux-gnu"
mkdir -p "$archive"
printf '#!/bin/sh\n' >"$archive/hamish"
cp "$root/packaging/linux/install.sh" "$root/packaging/linux/hamish.desktop" "$archive/"
cp -R "$root/packaging/icons" "$archive/icons"

prefix="$work/prefix with space"
PREFIX="$prefix" sh "$archive/install.sh"
for f in bin/hamish share/applications/hamish.desktop share/icons/hicolor/scalable/apps/hamish.svg \
  share/icons/hicolor/16x16/apps/hamish.png share/icons/hicolor/512x512/apps/hamish.png; do
  [ -f "$prefix/$f" ] || fail "not installed: $f"
done
[ -x "$prefix/bin/hamish" ] || fail "bin/hamish is not executable"
grep -qx "Exec=\"$prefix/bin/hamish\" %f" "$prefix/share/applications/hamish.desktop" ||
  fail "Exec does not point to the installed binary"
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$prefix/share/applications/hamish.desktop"
fi

PREFIX="$prefix" sh "$archive/install.sh" --uninstall
left=$(find "$prefix" -type f -name 'hamish*')
[ -z "$left" ] || fail "uninstall left: $left"

if PREFIX="$work/bad\$prefix" sh "$archive/install.sh" 2>/dev/null; then
  fail "install accepted a PREFIX with \$"
fi

mkdir -p "$work/repo/target/distrib" "$work/repo/packaging"
cp "$root/Cargo.toml" "$work/repo/"
cp -R "$root/packaging/macos" "$work/repo/packaging/macos"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  mkdir -p "$work/stage/hamish-$target"
  printf '#!/bin/sh\n' >"$work/stage/hamish-$target/hamish"
  tar -cJf "$work/repo/target/distrib/hamish-$target.tar.xz" -C "$work/stage" "hamish-$target"
done
sh "$work/repo/packaging/macos/package-apps.sh" >/dev/null
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  mkdir -p "$work/unzip/$target"
  (cd "$work/unzip/$target" && unzip -q "$work/repo/target/macos-app/Hamish-$target.app.zip")
  contents="$work/unzip/$target/Hamish.app/Contents"
  [ -x "$contents/MacOS/hamish" ] || fail "$target: MacOS/hamish missing or not executable"
  [ -f "$contents/Resources/Hamish.icns" ] || fail "$target: Hamish.icns missing"
  grep -q '<string>hamish</string>' "$contents/Info.plist" || fail "$target: CFBundleExecutable"
  if grep -q '@VERSION@' "$contents/Info.plist"; then
    fail "$target: version not set in Info.plist"
  fi
done

echo "packaging-test: ok"
