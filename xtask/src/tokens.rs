//! `cargo xtask tokens`: generate the app colour palette from the vendored design tokens
//! (`design/tokens/tokens.json`, ADR-032) and check WCAG contrast.
//!
//! - `tokens`: write `ui/theme/palette.rs` (colours), `ui/theme/scale.rs` (type scale and
//!   metrics) and `assets/themes/bitacora.json` (the "Bitacora Dark/Light" gpui-kit themes).
//! - `tokens --check`: fail when the generated file is stale (CI).
//! - `tokens --contrast`: fail when a text token drops below 4.5:1 on a surface.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value};

const TOKENS: &str = "design/tokens/tokens.json";
const PALETTE: &str = "crates/bitacora-app/src/ui/theme/palette.rs";

const TEXT: [&str; 6] = ["text", "text2", "muted", "accent", "ai", "warn"];
const SURFACES: [&str; 5] = ["bg", "side", "panel", "raised", "hover"];
const PAIRS: [(&str, &str); 2] = [("onAccent", "accent"), ("onAi", "ai")];

const SCALE: &str = "crates/bitacora-app/src/ui/theme/scale.rs";
const KIT_THEMES: &str = "crates/bitacora-app/assets/themes/bitacora.json";

const FONT_UI: &str = "Atkinson Hyperlegible Next";
const FONT_MONO: &str = "Atkinson Hyperlegible Mono";

/// A generated file: repo-relative path and its generator.
type Generator = fn(&Value) -> Result<String>;
const OUTPUTS: [(&str, Generator); 3] = [
    (PALETTE, generate),
    (SCALE, generate_scale),
    (KIT_THEMES, generate_kit_themes),
];

pub fn run(args: &[String]) -> Result<bool> {
    let root = repo_root()?;
    let tokens = read_tokens(&root.join(TOKENS))?;
    match args.first().map(String::as_str) {
        None => {
            for (path, generator) in OUTPUTS {
                let target = root.join(path);
                std::fs::write(&target, generator(&tokens)?)
                    .with_context(|| format!("writing {}", target.display()))?;
                println!("wrote {path}");
            }
            Ok(true)
        }
        Some("--check") => {
            let mut ok = true;
            for (path, generator) in OUTPUTS {
                let want = generator(&tokens)?;
                let have = std::fs::read_to_string(root.join(path)).unwrap_or_default();
                if have == want {
                    println!("{path} is up to date");
                } else {
                    eprintln!("{path} is stale: run `cargo xtask tokens` and commit the result");
                    ok = false;
                }
            }
            Ok(ok)
        }
        Some("--contrast") => contrast(&tokens),
        Some(other) => bail!("unknown `tokens` option `{other}` (use --check or --contrast)"),
    }
}

fn repo_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("xtask manifest has no parent directory"))
}

fn read_tokens(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn modes(tokens: &Value) -> Result<&Map<String, Value>> {
    tokens
        .get("color")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("tokens.json has no `color` object"))
}

fn mode<'a>(colors: &'a Map<String, Value>, name: &str) -> Result<&'a Map<String, Value>> {
    colors
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("tokens.json has no `color.{name}`"))
}

fn value_of(group: &Map<String, Value>, name: &str) -> Result<String> {
    let hex = group
        .get(name)
        .and_then(|t| t.get("$value"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("token `{name}` has no string $value"))?
        .trim_start_matches('#')
        .to_ascii_uppercase();
    if (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hex)
    } else {
        bail!("token `{name}` is not #RRGGBB or #RRGGBBAA: {hex}")
    }
}

/// `editBg` -> `edit_bg`, `text2` -> `text_2`, `line2` -> `line_2`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        // A digit starts a new word only at the end of the name (`text2` -> `text_2`), so
        // `h1Block` becomes `h1_block`.
        let digit_word = c.is_ascii_digit() && chars[i + 1..].iter().all(char::is_ascii_digit);
        if i > 0 && (c.is_ascii_uppercase() || digit_word) {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn generate(tokens: &Value) -> Result<String> {
    let colors = modes(tokens)?;
    let dark = mode(colors, "dark")?;
    let light = mode(colors, "light")?;
    let names: Vec<&String> = dark.keys().collect();
    if !light.keys().eq(dark.keys()) {
        bail!("`color.dark` and `color.light` must define the same tokens in the same order");
    }

    let mut rs = String::new();
    rs.push_str(
        "// GENERATED by `cargo xtask tokens` from design/tokens/tokens.json. Do not edit.\n",
    );
    rs.push_str("use crate::ui::{Hsla, rgb, rgba};\n\n");
    rs.push_str("/// Semantic Bitacora colours (design system tokens).\n");
    rs.push_str("#[derive(Clone, Copy, Debug, PartialEq)]\npub struct Palette {\n");
    for n in &names {
        if let Some(d) = dark[*n].get("$description").and_then(Value::as_str) {
            writeln!(rs, "    /// {d}")?;
        }
        writeln!(rs, "    pub {}: Hsla,", snake(n))?;
    }
    rs.push_str("}\n\nimpl Palette {\n");
    for (fname, group) in [("dark", dark), ("light", light)] {
        writeln!(rs, "    pub fn {fname}() -> Self {{\n        Self {{")?;
        for n in &names {
            let hex = value_of(group, n)?;
            let ctor = if hex.len() == 6 { "rgb" } else { "rgba" };
            writeln!(rs, "            {}: {ctor}(0x{hex}).into(),", snake(n))?;
        }
        rs.push_str("        }\n    }\n\n");
    }
    writeln!(
        rs,
        "    /// Every token as `(tokens.json key, colour)`, in declaration order.\n    pub fn entries(&self) -> [(&'static str, Hsla); {}] {{\n        [",
        names.len()
    )?;
    for n in &names {
        writeln!(rs, "            (\"{n}\", self.{}),", snake(n))?;
    }
    rs.push_str("        ]\n    }\n}\n");
    Ok(rs)
}

/// Metrics the design system defines in `rust/theme.rs` but not in `tokens.json`:
/// `(field, px, doc)`.
const EXTRA_METRICS: [(&str, f64, &str); 7] = [
    ("reading_pad_top", 44.0, "Reading column padding, top"),
    ("reading_pad_x", 40.0, "Reading column padding, sides"),
    ("reading_pad_bottom", 80.0, "Reading column padding, bottom"),
    ("journal_gap", 56.0, "Gap between consecutive journals"),
    ("block_gap", 6.0, "Vertical gap between sibling blocks"),
    (
        "child_guide_margin",
        8.0,
        "Children: margin up to the guide line",
    ),
    (
        "child_guide_pad",
        18.0,
        "Children: padding after the guide line",
    ),
];

fn px_of(group: &Map<String, Value>, name: &str) -> Result<f64> {
    group
        .get(name)
        .and_then(|t| t.get("$value"))
        .and_then(Value::as_str)
        .and_then(|v| v.strip_suffix("px"))
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| anyhow!("token `{name}` is not a px dimension"))
}

fn object<'a>(tokens: &'a Value, name: &str) -> Result<&'a Map<String, Value>> {
    tokens
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("tokens.json has no `{name}` object"))
}

/// Rust float literal (`34.0`, `14.5`).
fn float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

fn family_const(reference: &str) -> Result<&'static str> {
    match reference {
        "{font.family.ui}" => Ok("FONT_UI"),
        "{font.family.display}" => Ok("FONT_DISPLAY"),
        "{font.family.mono}" => Ok("FONT_MONO"),
        other => bail!("unknown font family reference `{other}`"),
    }
}

/// One typography token as Rust `TypeStyle` source.
fn type_style(name: &str, t: &Value) -> Result<String> {
    let v = t
        .get("$value")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("typography `{name}` has no $value"))?;
    let field = |key: &str| {
        v.get(key)
            .ok_or_else(|| anyhow!("typography `{name}` has no {key}"))
    };
    let family = family_const(
        field("fontFamily")?
            .as_str()
            .ok_or_else(|| anyhow!("typography `{name}`: fontFamily is not a string"))?,
    )?;
    let size: f64 = field("fontSize")?
        .as_str()
        .and_then(|s| s.strip_suffix("px"))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("typography `{name}`: fontSize is not px"))?;
    let weight = field("fontWeight")?
        .as_f64()
        .ok_or_else(|| anyhow!("typography `{name}`: fontWeight is not a number"))?;
    let line_height = field("lineHeight")?
        .as_f64()
        .ok_or_else(|| anyhow!("typography `{name}`: lineHeight is not a number"))?;
    let tracking: f64 = match v.get("letterSpacing").and_then(Value::as_str) {
        Some(s) => s
            .strip_suffix("em")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("typography `{name}`: letterSpacing is not em"))?,
        None => 0.0,
    };
    let uppercase = v.get("textTransform").and_then(Value::as_str) == Some("uppercase");
    Ok(format!(
        "            {}: TypeStyle {{\n                family: {family}.into(),\n                size: px({}),\n                weight: FontWeight({}),\n                line_height: {},\n                tracking_em: {},\n                uppercase: {uppercase},\n            }},\n",
        snake(name),
        float(size),
        float(weight),
        float(line_height),
        float(tracking)
    ))
}

/// Type scale and metrics (`ui/theme/scale.rs`).
fn generate_scale(tokens: &Value) -> Result<String> {
    let typography = object(tokens, "typography")?;
    let size = object(tokens, "size")?;
    let radius = object(tokens, "radius")?;
    let space = object(tokens, "space")?;
    let is_number = |t: &Value| t.get("$type").and_then(Value::as_str) == Some("number");

    let mut rs = String::new();
    rs.push_str(
        "// GENERATED by `cargo xtask tokens` from design/tokens/tokens.json. Do not edit.\n",
    );
    rs.push_str("use crate::fonts::{FONT_DISPLAY, FONT_MONO, FONT_UI};\n");
    rs.push_str("use crate::ui::text_edit::FontWeight;\n");
    rs.push_str("use crate::ui::{Pixels, SharedString, px};\n\n");
    rs.push_str("/// One text style of the type scale.\n#[derive(Clone, Debug, PartialEq)]\npub struct TypeStyle {\n    pub family: SharedString,\n    pub size: Pixels,\n    pub weight: FontWeight,\n    /// Line height relative to the size (1.65 = 165 %).\n    pub line_height: f32,\n    /// Letter spacing in em (-0.01 = -1 %).\n    pub tracking_em: f32,\n    pub uppercase: bool,\n}\n\n");
    rs.push_str("/// The design system type scale.\n#[derive(Clone, Debug, PartialEq)]\npub struct TypeScale {\n");
    for (name, t) in typography {
        if let Some(d) = t.get("$description").and_then(Value::as_str) {
            writeln!(rs, "    /// {d}")?;
        }
        writeln!(rs, "    pub {}: TypeStyle,", snake(name))?;
    }
    rs.push_str("}\n\nimpl TypeScale {\n    /// Every style as `(tokens.json key, style)`, in declaration order.\n");
    writeln!(
        rs,
        "    pub fn entries(&self) -> [(&'static str, &TypeStyle); {}] {{\n        [",
        typography.len()
    )?;
    for name in typography.keys() {
        writeln!(rs, "            (\"{name}\", &self.{}),", snake(name))?;
    }
    rs.push_str(
        "        ]\n    }\n}\n\nimpl Default for TypeScale {\n    fn default() -> Self {\n        Self {\n",
    );
    for (name, t) in typography {
        rs.push_str(&type_style(name, t)?);
    }
    rs.push_str("        }\n    }\n}\n\n");

    rs.push_str("/// Layout metrics of the design system (sizes, radii, spacing).\n#[derive(Clone, Debug, PartialEq)]\npub struct Metrics {\n");
    for (name, t) in size {
        if let Some(d) = t.get("$description").and_then(Value::as_str) {
            writeln!(rs, "    /// {d}")?;
        }
        let ty = if is_number(t) { "f32" } else { "Pixels" };
        writeln!(rs, "    pub {}: {ty},", snake(name))?;
    }
    for name in radius.keys() {
        writeln!(rs, "    pub radius_{}: Pixels,", snake(name))?;
    }
    for (name, _, doc) in EXTRA_METRICS {
        writeln!(rs, "    /// {doc}")?;
        writeln!(rs, "    pub {name}: Pixels,")?;
    }
    writeln!(
        rs,
        "    /// Spacing scale (`space.0` to `space.{}`).\n    pub space: [Pixels; {}],\n}}\n",
        space.len() - 1,
        space.len()
    )?;
    rs.push_str("impl Default for Metrics {\n    fn default() -> Self {\n        Self {\n");
    for (name, t) in size {
        if is_number(t) {
            let v = t
                .get("$value")
                .and_then(Value::as_f64)
                .ok_or_else(|| anyhow!("size `{name}` is not a number"))?;
            writeln!(rs, "            {}: {},", snake(name), float(v))?;
        } else {
            let v = px_of(size, name)?;
            writeln!(rs, "            {}: px({}),", snake(name), float(v))?;
        }
    }
    for name in radius.keys() {
        let v = px_of(radius, name)?;
        writeln!(rs, "            radius_{}: px({}),", snake(name), float(v))?;
    }
    for (name, value, _) in EXTRA_METRICS {
        writeln!(rs, "            {name}: px({}),", float(value))?;
    }
    rs.push_str("            space: [\n");
    for name in space.keys() {
        writeln!(rs, "                px({}),", float(px_of(space, name)?))?;
    }
    rs.push_str("            ],\n        }\n    }\n}\n");
    Ok(rs)
}

/// How a kit colour is taken from the palette: a token, optionally forced to an alpha (0-255).
type KitSource = (&'static str, Option<u8>);

/// gpui-component 0.7.1 `ThemeConfig` colour keys and the design token each one takes
/// (design system `docs/gpui-kit.md`; the app test `kit_theme_keys_exist` checks every key
/// against the crate's `ThemeConfigColors`).
const KIT_MAP: &[(&str, KitSource)] = &[
    ("background", ("bg", None)),
    ("foreground", ("text", None)),
    ("border", ("line", None)),
    ("input.border", ("line2", None)),
    ("muted.background", ("hover", None)),
    ("muted.foreground", ("muted", None)),
    ("accent.background", ("hover", None)),
    ("accent.foreground", ("text", None)),
    ("primary.background", ("accent", None)),
    ("primary.foreground", ("onAccent", None)),
    ("secondary.background", ("raised", None)),
    ("secondary.foreground", ("text", None)),
    ("link", ("accent", None)),
    ("link.hover", ("accent", None)),
    ("link.active", ("accent", None)),
    ("selection.background", ("accent", Some(0x4D))),
    ("ring", ("accent", None)),
    ("caret", ("accent", None)),
    ("popover.background", ("raised", None)),
    ("popover.foreground", ("text", None)),
    ("sidebar.background", ("side", None)),
    ("sidebar.foreground", ("text2", None)),
    ("sidebar.border", ("line", None)),
    ("sidebar.accent.background", ("hover", None)),
    ("sidebar.accent.foreground", ("text", None)),
    ("sidebar.primary.background", ("accent", None)),
    ("sidebar.primary.foreground", ("onAccent", None)),
    ("title_bar.background", ("side", None)),
    ("title_bar.border", ("line", None)),
    ("status_bar.background", ("side", None)),
    ("status_bar.border", ("line", None)),
    ("tab_bar.background", ("side", None)),
    ("tab.background", ("side", None)),
    ("tab.foreground", ("muted", None)),
    ("tab.active.background", ("bg", None)),
    ("tab.active.foreground", ("text", None)),
    ("list.background", ("bg", None)),
    ("list.even.background", ("bg", None)),
    ("list.head.background", ("panel", None)),
    ("list.hover.background", ("hover", None)),
    ("list.active.background", ("hover", None)),
    ("list.active.border", ("accent", None)),
    ("table.background", ("bg", None)),
    ("table.even.background", ("bg", None)),
    ("table.head.background", ("panel", None)),
    ("table.head.foreground", ("text2", None)),
    ("table.hover.background", ("hover", None)),
    ("table.active.background", ("hover", None)),
    ("table.active.border", ("accent", None)),
    ("table.row.border", ("line", None)),
    ("scrollbar.background", ("bg", None)),
    ("scrollbar.thumb.background", ("line2", None)),
    ("scrollbar.thumb.hover.background", ("muted", None)),
    ("danger.background", ("warn", None)),
    ("danger.foreground", ("onAccent", None)),
    ("warning.background", ("warn", None)),
    ("warning.foreground", ("onAccent", None)),
    ("success.background", ("ok", None)),
    ("success.foreground", ("onAccent", None)),
    ("info.background", ("accent", None)),
    ("info.foreground", ("onAccent", None)),
    ("progress.bar.background", ("accent", None)),
    ("slider.background", ("accent", None)),
    ("slider.thumb.background", ("accent", None)),
    ("switch.background", ("line2", None)),
    ("switch.thumb.background", ("raised", None)),
    ("skeleton.background", ("hover", None)),
    ("drop_target.background", ("accentBg", None)),
    ("drag.border", ("accent", None)),
    ("description_list.label.background", ("panel", None)),
    ("description_list.label.foreground", ("text2", None)),
];

/// The "Bitacora Light" / "Bitacora Dark" kit themes (`assets/themes/bitacora.json`).
fn generate_kit_themes(tokens: &Value) -> Result<String> {
    let colors = modes(tokens)?;
    let mut out = String::new();
    out.push_str("{\n  \"name\": \"Bitacora\",\n  \"author\": \"Bitacora (generated by `cargo xtask tokens`)\",\n  \"themes\": [\n");
    for (i, (mode_name, title)) in [("light", "Bitacora Light"), ("dark", "Bitacora Dark")]
        .into_iter()
        .enumerate()
    {
        let group = mode(colors, mode_name)?;
        if i > 0 {
            out.push_str(",\n");
        }
        writeln!(
            out,
            "    {{\n      \"name\": \"{title}\",\n      \"mode\": \"{mode_name}\",\n      \"font.family\": \"{FONT_UI}\",\n      \"mono_font.family\": \"{FONT_MONO}\",\n      \"colors\": {{"
        )?;
        for (j, (key, (token, alpha))) in KIT_MAP.iter().enumerate() {
            let hex = value_of(group, token)?;
            let alpha_hex = match alpha {
                Some(a) => format!("{a:02X}"),
                None => hex[6..].to_owned(),
            };
            let sep = if j + 1 == KIT_MAP.len() { "" } else { "," };
            writeln!(out, "        \"{key}\": \"#{}{alpha_hex}\"{sep}", &hex[..6])?;
        }
        out.push_str("      }\n    }");
    }
    out.push_str("\n  ]\n}\n");
    Ok(out)
}

fn luminance(hex: &str) -> f64 {
    let channel = |i: usize| {
        let v = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn contrast(tokens: &Value) -> Result<bool> {
    let mut ok = true;
    for (mode_name, group) in modes(tokens)? {
        let group = group
            .as_object()
            .ok_or_else(|| anyhow!("`color.{mode_name}` is not an object"))?;
        let mut checks: Vec<(&str, &str)> = Vec::new();
        for fg in TEXT {
            for bg in SURFACES {
                checks.push((fg, bg));
            }
        }
        checks.extend(PAIRS);
        for (fg, bg) in checks {
            let r = ratio(&value_of(group, fg)?, &value_of(group, bg)?);
            let pass = r >= 4.5;
            ok &= pass;
            println!(
                "{} {mode_name:5} {fg:9} on {bg:7} {r:5.1}:1",
                if pass { "OK " } else { "LOW" }
            );
        }
    }
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_matches_upstream_generator() {
        assert_eq!(snake("editBg"), "edit_bg");
        assert_eq!(snake("text2"), "text_2");
        assert_eq!(snake("onAccent"), "on_accent");
        assert_eq!(snake("h1Block"), "h1_block");
    }

    #[test]
    fn vendored_tokens_pass_contrast() {
        let root = repo_root().expect("root");
        let tokens = read_tokens(&root.join(TOKENS)).expect("tokens");
        assert!(contrast(&tokens).expect("contrast"));
    }
}
