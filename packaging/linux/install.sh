#!/bin/sh
# Installs Hamish as a desktop app for the current user, or removes it.
# Usage: install.sh [--uninstall]
# PREFIX sets the install root (default: ~/.local).
# HAMISH_BIN, HAMISH_DESKTOP and HAMISH_ICONS set the source files
# (default: hamish, hamish.desktop and icons/ next to this script).
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local}
bin_src=${HAMISH_BIN:-$here/hamish}
desktop_src=${HAMISH_DESKTOP:-$here/hamish.desktop}
icons_src=${HAMISH_ICONS:-$here/icons}

bin_dir="$prefix/bin"
apps_dir="$prefix/share/applications"
hicolor="$prefix/share/icons/hicolor"
sizes="16 24 32 48 64 128 256 512"

refresh_caches() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$apps_dir" || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$hicolor" ]; then
    gtk-update-icon-cache -q -t -f "$hicolor" || true
  fi
}

uninstall() {
  rm -f "$bin_dir/hamish" "$apps_dir/hamish.desktop" "$hicolor/scalable/apps/hamish.svg"
  for n in $sizes; do
    rm -f "$hicolor/${n}x${n}/apps/hamish.png"
  done
  refresh_caches
  echo "Hamish removed from $prefix"
}

install_app() {
  for f in "$bin_src" "$desktop_src" "$icons_src/hamish.svg"; do
    if [ ! -f "$f" ]; then
      echo "install.sh: missing $f" >&2
      exit 1
    fi
  done
  case "$bin_dir" in
    *[\\\"\`\$%\&\|]*)
      echo "install.sh: PREFIX must not contain \\ \" \` \$ % & |" >&2
      exit 1
      ;;
  esac
  mkdir -p "$bin_dir" "$apps_dir" "$hicolor/scalable/apps"
  cp "$bin_src" "$bin_dir/hamish.tmp"
  chmod 755 "$bin_dir/hamish.tmp"
  mv -f "$bin_dir/hamish.tmp" "$bin_dir/hamish"
  # An absolute Exec path: ~/.local/bin is not always on the PATH of the desktop session.
  sed "s|^Exec=hamish |Exec=\"$bin_dir/hamish\" |" "$desktop_src" >"$apps_dir/hamish.desktop"
  chmod 644 "$apps_dir/hamish.desktop"
  cp "$icons_src/hamish.svg" "$hicolor/scalable/apps/hamish.svg"
  for n in $sizes; do
    mkdir -p "$hicolor/${n}x${n}/apps"
    cp "$icons_src/hicolor/${n}x${n}/apps/hamish.png" "$hicolor/${n}x${n}/apps/hamish.png"
  done
  refresh_caches
  echo "Hamish installed in $prefix"
}

if [ "$(uname -s)" = Darwin ]; then
  echo "install.sh: this script is for Linux. On macOS, use Hamish.app from the release." >&2
  exit 1
fi

case "${1:-}" in
  "") install_app ;;
  --uninstall) uninstall ;;
  *)
    echo "usage: install.sh [--uninstall]" >&2
    exit 2
    ;;
esac
