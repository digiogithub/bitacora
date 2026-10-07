#!/usr/bin/env python3
"""Comprueba el contraste WCAG de los tokens de texto sobre cada superficie.

Uso:  python3 tools/check_contrast.py      (sale con código 1 si algo baja de 4.5:1)
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
colors = json.loads((ROOT / "tokens" / "tokens.json").read_text(encoding="utf-8"))["color"]

TEXT = ("text", "text2", "muted", "accent", "ai", "warn")
SURFACES = ("bg", "side", "panel", "raised", "hover")
PAIRS = (("onAccent", "accent"), ("onAi", "ai"))


def lum(h: str) -> float:
    h = h.lstrip("#")[:6]
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    c = [x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def ratio(a: str, b: str) -> float:
    la, lb = lum(a), lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


fail = False
for mode, toks in colors.items():
    v = {k: t["$value"] for k, t in toks.items()}
    checks = [(fg, bg) for fg in TEXT for bg in SURFACES] + list(PAIRS)
    for fg, bg in checks:
        r = ratio(v[fg], v[bg])
        flag = "OK " if r >= 4.5 else "BAJO"
        if r < 4.5:
            fail = True
        print(f"{flag} {mode:5} {fg:9} sobre {bg:7} {r:5.1f}:1")
sys.exit(1 if fail else 0)
