#!/usr/bin/env bash
# Regenerates the PNG ramp and the multi-resolution .ico from tomtemacro.svg.
# The exported files are committed so ordinary builds never need this script
# (or inkscape/ImageMagick). Run it only after editing the SVG.
#
# The macOS .icns is NOT built here: iconutil only exists on macOS, so the
# release workflow assembles it on the macos runner from the same PNG ramp.
set -euo pipefail
cd "$(dirname "$0")"

command -v inkscape >/dev/null || { echo "inkscape is required" >&2; exit 1; }
command -v convert >/dev/null || { echo "ImageMagick (convert) is required" >&2; exit 1; }

sizes=(16 24 32 48 64 128 256 512 1024)
mkdir -p png
for s in "${sizes[@]}"; do
  inkscape tomtemacro.svg -w "$s" -h "$s" -o "png/tomtemacro-$s.png"
  convert "png/tomtemacro-$s.png" -strip "png/tomtemacro-$s.png"
done

# 256 is stored PNG-compressed inside the .ico (Vista+ convention).
convert png/tomtemacro-{16,24,32,48,64,128,256}.png tomtemacro.ico

echo "Done:"
ls -la png/ tomtemacro.ico
