#!/usr/bin/env bash
# Regenerate the raster icons (PNG set, .ico, .icns) from packaging/icons/bitacora.svg.
# Needs inkscape and icotool (icoutils). The generated files are committed.
set -euo pipefail
cd "$(dirname "$0")/icons"
for s in 16 32 48 64 128 256 512 1024; do
  inkscape bitacora.svg -w "$s" -h "$s" -o "${s}x${s}.png" >/dev/null 2>&1
done
icotool -c -o bitacora.ico 16x16.png 32x32.png 48x48.png 64x64.png 128x128.png 256x256.png
python3 -I - <<'PY'
import struct
parts = [(b"ic07", "128x128.png"), (b"ic08", "256x256.png"),
         (b"ic09", "512x512.png"), (b"ic10", "1024x1024.png")]
body = b""
for kind, name in parts:
    data = open(name, "rb").read()
    body += kind + struct.pack(">I", len(data) + 8) + data
open("bitacora.icns", "wb").write(b"icns" + struct.pack(">I", len(body) + 8) + body)
PY
