#!/bin/sh
# Usage: package-vsix.sh <vsce target> <typst-redline binary>
# Writes typst-redline-<target>.vsix with the binary in bin/, and fails if a Unix binary lost its executable bit.
set -eu
target=$1
case $target in win32-*) name=typst-redline.exe ;; *) name=typst-redline ;; esac
vsix=typst-redline-$target.vsix

mkdir -p bin
cp "$2" "bin/$name"
chmod +x "bin/$name"
npx --no-install vsce package --no-dependencies --target "$target" -o "$vsix"
rm -rf bin

unzip -Z1 "$vsix" | grep -qx extension/LICENSE.txt || { echo "no licence in $vsix" >&2; exit 1; }
case $target in
  win32-*) ;;
  *) unzip -Z "$vsix" "extension/bin/$name" | grep -q '^-rwx' || { echo "$name in $vsix is not executable" >&2; exit 1; } ;;
esac
