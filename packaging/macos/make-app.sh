#!/bin/sh
# Builds an unsigned Hamish.app from a hamish binary.
# Usage: make-app.sh <hamish-binary> <out-dir> <version>
set -eu
if [ "$#" -ne 3 ]; then
  echo "usage: make-app.sh <hamish-binary> <out-dir> <version>" >&2
  exit 2
fi
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary=$1
app="$2/Hamish.app"
version=$3

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/hamish"
chmod 755 "$app/Contents/MacOS/hamish"
cp "$here/Hamish.icns" "$app/Contents/Resources/Hamish.icns"
sed "s/@VERSION@/$version/g" "$here/Info.plist" >"$app/Contents/Info.plist"
printf 'APPL????' >"$app/Contents/PkgInfo"
echo "$app"
