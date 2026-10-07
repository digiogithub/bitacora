#!/usr/bin/env python3
"""Genera tokens.css y rust/palette.rs a partir de tokens/tokens.json.

Uso:  python3 tools/gen_tokens.py
Edita SIEMPRE tokens.json y regenera; no edites a mano los ficheros generados.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
tokens = json.loads((ROOT / "tokens" / "tokens.json").read_text(encoding="utf-8"))
colors = tokens["color"]
NAMES = list(colors["dark"].keys())


def snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z0-9])", "_", name).lower()


def hex_of(mode: str, name: str) -> str:
    return colors[mode][name]["$value"].lstrip("#").upper()


def css_color(h: str) -> str:
    if len(h) == 6:
        return "#" + h.lower()
    r, g, b, a = (int(h[i:i + 2], 16) for i in (0, 2, 4, 6))
    return f"rgba({r}, {g}, {b}, {a / 255:.2f})"


# ---------- tokens.css ----------
css = ["/* GENERADO por tools/gen_tokens.py — no editar a mano */", ""]
css.append(":root,")
css.append('[data-theme="dark"] {')
for n in NAMES:
    css.append(f"  --{snake(n).replace('_', '-')}: {css_color(hex_of('dark', n))};")
css.append("}")
css.append("")
css.append('[data-theme="light"] {')
for n in NAMES:
    css.append(f"  --{snake(n).replace('_', '-')}: {css_color(hex_of('light', n))};")
css.append("}")
css.append("")
css.append("@media print {")
css.append("  :root {")
for n in NAMES:
    css.append(f"    --{snake(n).replace('_', '-')}: {css_color(hex_of('light', n))};")
css.append("  }")
css.append("}")
css.append("")
fam = tokens["font"]["family"]
css.append(":root {")
for k, v in fam.items():
    stack = ", ".join(f"'{f}'" if " " in f else f for f in v["$value"])
    css.append(f"  --font-{k}: {stack};")
for k, v in tokens["radius"].items():
    css.append(f"  --radius-{snake(k).replace('_', '-')}: {v['$value']};")
for k, v in tokens["space"].items():
    css.append(f"  --space-{k}: {v['$value']};")
for k, v in tokens["size"].items():
    val = v["$value"]
    css.append(f"  --size-{snake(k).replace('_', '-')}: {val};")
css.append("}")
(ROOT / "tokens" / "tokens.css").write_text("\n".join(css) + "\n", encoding="utf-8")

# ---------- rust/palette.rs ----------
rs = [
    "// GENERADO por tools/gen_tokens.py a partir de tokens/tokens.json — no editar a mano.",
    "use gpui::{rgb, rgba, Hsla};",
    "",
    "/// Colores semánticos de Bitácora. Ver docs/fundamentos.md para el uso de cada token.",
    "#[derive(Clone, Copy, Debug, PartialEq)]",
    "pub struct Palette {",
]
for n in NAMES:
    desc = colors["dark"][n].get("$description")
    if desc:
        rs.append(f"    /// {desc}")
    rs.append(f"    pub {snake(n)}: Hsla,")
rs.append("}")
rs.append("")


def rust_expr(h: str) -> str:
    if len(h) == 6:
        return f"rgb(0x{h}).into()"
    return f"rgba(0x{h}).into()"


rs.append("impl Palette {")
for mode in ("dark", "light"):
    rs.append(f"    pub fn {mode}() -> Self {{")
    rs.append("        Self {")
    for n in NAMES:
        rs.append(f"            {snake(n)}: {rust_expr(hex_of(mode, n))},")
    rs.append("        }")
    rs.append("    }")
    if mode == "dark":
        rs.append("")
rs.append("}")
(ROOT / "rust" / "palette.rs").write_text("\n".join(rs) + "\n", encoding="utf-8")

print("OK: tokens/tokens.css y rust/palette.rs regenerados")
