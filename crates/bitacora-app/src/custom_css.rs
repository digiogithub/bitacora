//! A tolerant reader for the documented subset of a graph's `logseq/custom.css` (BIT-T-0334).
//!
//! Logseq themes are arbitrary CSS against Logseq's own DOM, which Bitacora does not have. We
//! therefore read only what maps cleanly onto the GPUI Kit theme: a handful of `--ls-*`
//! variables and the font family / size set on `:root`, `html`, `body` or `.editor`. Everything
//! else is ignored and reported as a [`Diagnostic`] so the user sees what was not applied
//! (Settings > Appearance). The file is only ever read, never written (ADR-001 user files are
//! sacred).
//!
//! The parser is deliberately small: comments are blanked, `{}` blocks are matched with string
//! awareness, and a malformed construct yields a diagnostic instead of an error.

use std::collections::BTreeMap;

/// An RGBA colour, 8 bits per channel.
pub type Rgba8 = [u8; 4];

/// Why a piece of the stylesheet was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// An at-rule (`@media`, `@import`, `@font-face`...).
    AtRule,
    /// A selector other than the supported scopes.
    Selector,
    /// A property or `--ls-*` variable outside the documented subset.
    Property,
    /// A supported property whose value could not be understood.
    Value,
}

impl Reason {
    /// The locale key of the human description.
    pub fn key(self) -> &'static str {
        match self {
            Self::AtRule => "settings.appearance.css_reason_at_rule",
            Self::Selector => "settings.appearance.css_reason_selector",
            Self::Property => "settings.appearance.css_reason_property",
            Self::Value => "settings.appearance.css_reason_value",
        }
    }
}

/// One ignored construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line of the construct.
    pub line: usize,
    /// The selector, at-rule or property that was ignored.
    pub subject: String,
    /// Why.
    pub reason: Reason,
}

/// The values one scope sets; every field is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    /// `--ls-primary-background-color`.
    pub background: Option<Rgba8>,
    /// `--ls-secondary-background-color` (sidebars).
    pub secondary_background: Option<Rgba8>,
    /// `--ls-tertiary-background-color` (muted surfaces).
    pub tertiary_background: Option<Rgba8>,
    /// `--ls-primary-text-color`.
    pub text: Option<Rgba8>,
    /// `--ls-secondary-text-color`.
    pub secondary_text: Option<Rgba8>,
    /// `--ls-link-text-color`.
    pub link: Option<Rgba8>,
    /// `--ls-block-bullet-color`.
    pub bullet: Option<Rgba8>,
    /// `--ls-border-color`.
    pub border: Option<Rgba8>,
    /// `--ls-selection-background-color`.
    pub selection: Option<Rgba8>,
    /// `font-family` / `--ls-font-family`: the first concrete family.
    pub font_family: Option<String>,
    /// `font-size` in pixels.
    pub font_size: Option<f32>,
}

impl Overrides {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Lets `other` win field by field.
    fn overlay(&mut self, other: &Self) {
        macro_rules! take {
            ($($f:ident),*) => {$( if other.$f.is_some() { self.$f = other.$f.clone(); } )*};
        }
        take!(
            background,
            secondary_background,
            tertiary_background,
            text,
            secondary_text,
            link,
            bullet,
            border,
            selection,
            font_family,
            font_size
        );
    }

    fn count(&self) -> usize {
        [
            self.background,
            self.secondary_background,
            self.tertiary_background,
            self.text,
            self.secondary_text,
            self.link,
            self.bullet,
            self.border,
            self.selection,
        ]
        .iter()
        .filter(|c| c.is_some())
        .count()
            + usize::from(self.font_family.is_some())
            + usize::from(self.font_size.is_some())
    }
}

/// The parsed stylesheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomCss {
    /// Applies in every theme mode.
    pub base: Overrides,
    /// Applies in light mode only (`.white-theme`, `[data-theme=light]`).
    pub light: Overrides,
    /// Applies in dark mode only (`.dark-theme`, `[data-theme=dark]`).
    pub dark: Overrides,
    /// What was ignored.
    pub diagnostics: Vec<Diagnostic>,
}

impl CustomCss {
    /// The overrides in force for the given mode.
    pub fn effective(&self, dark: bool) -> Overrides {
        let mut out = self.base.clone();
        out.overlay(if dark { &self.dark } else { &self.light });
        out
    }

    /// How many values were understood across all scopes.
    pub fn applied_count(&self) -> usize {
        self.base.count() + self.light.count() + self.dark.count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Base,
    Light,
    Dark,
}

fn classify(selector: &str) -> Option<Scope> {
    let norm: String = selector
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\'' && *c != '"')
        .collect::<String>()
        .to_ascii_lowercase();
    match norm.as_str() {
        ":root" | "html" | "body" | ".editor" | "html,body" => Some(Scope::Base),
        ".white-theme"
        | ":root.white-theme"
        | "html.white-theme"
        | ":root[data-theme=light]"
        | "html[data-theme=light]"
        | "body[data-theme=light]" => Some(Scope::Light),
        ".dark-theme"
        | ":root.dark-theme"
        | "html.dark-theme"
        | ":root[data-theme=dark]"
        | "html[data-theme=dark]"
        | "body[data-theme=dark]" => Some(Scope::Dark),
        _ => None,
    }
}

/// Parses `css`; never fails.
pub fn parse(css: &str) -> CustomCss {
    let text = blank_comments(css);
    let mut out = CustomCss::default();
    let mut vars: [BTreeMap<String, String>; 3] = Default::default();
    // (scope, line, property, value), resolved after all variables are known.
    let mut decls: Vec<(Scope, usize, String, String)> = Vec::new();
    walk_rules(&text, &mut out, &mut vars, &mut decls);

    for (scope, line, prop, value) in decls {
        let idx = scope_index(scope);
        // `var()` resolves against the scope first, then the base.
        let lookup = |name: &str| {
            vars[idx]
                .get(name)
                .or_else(|| vars[0].get(name))
                .map(String::as_str)
        };
        let target = match scope {
            Scope::Base => &mut out.base,
            Scope::Light => &mut out.light,
            Scope::Dark => &mut out.dark,
        };
        apply_declaration(target, &prop, &value, &lookup, line, &mut out.diagnostics);
    }
    out.diagnostics.sort_by_key(|d| d.line);
    out
}

fn scope_index(scope: Scope) -> usize {
    match scope {
        Scope::Base => 0,
        Scope::Light => 1,
        Scope::Dark => 2,
    }
}

/// Replaces comments with spaces, keeping newlines so line numbers survive.
fn blank_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else if c == q || c == '\n' {
                    quote = None;
                }
            }
            None if c == '/' && chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
                out.push(' ');
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

/// Index of the `}` matching the `{` before `from`, or the end of the text when unbalanced.
fn matching_brace(text: &str, from: usize) -> usize {
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in text[from..].char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q || c == '\n' {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return from + i;
                }
            }
            _ => {}
        }
    }
    text.len()
}

fn line_of(text: &str, offset: usize) -> usize {
    1 + text[..offset].bytes().filter(|b| *b == b'\n').count()
}

fn short(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 60 {
        format!("{}...", s.chars().take(57).collect::<String>())
    } else {
        s
    }
}

fn walk_rules(
    text: &str,
    out: &mut CustomCss,
    vars: &mut [BTreeMap<String, String>; 3],
    decls: &mut Vec<(Scope, usize, String, String)>,
) {
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        let skipped = rest.len() - rest.trim_start().len();
        pos += skipped;
        if pos >= text.len() {
            break;
        }
        let rest = &text[pos..];
        // The prelude ends at `{` (a rule) or `;` (a statement at-rule such as `@import`).
        let Some(end) = rest.find(['{', ';']) else {
            // Trailing garbage without a block.
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                out.diagnostics.push(Diagnostic {
                    line: line_of(text, pos),
                    subject: short(trimmed),
                    reason: Reason::Selector,
                });
            }
            break;
        };
        let prelude = rest[..end].trim();
        let line = line_of(
            text,
            pos + rest[..end].len() - rest[..end].trim_start().len(),
        );
        if rest.as_bytes()[end] == b';' {
            if !prelude.is_empty() {
                out.diagnostics.push(Diagnostic {
                    line,
                    subject: short(prelude),
                    reason: if prelude.starts_with('@') {
                        Reason::AtRule
                    } else {
                        Reason::Selector
                    },
                });
            }
            pos += end + 1;
            continue;
        }
        let body_start = pos + end + 1;
        let body_end = matching_brace(text, body_start);
        let body = &text[body_start..body_end];
        pos = (body_end + 1).min(text.len());

        if prelude.starts_with('@') {
            out.diagnostics.push(Diagnostic {
                line,
                subject: short(prelude),
                reason: Reason::AtRule,
            });
            continue;
        }
        let mut scopes = Vec::new();
        for selector in prelude.split(',') {
            match classify(selector) {
                Some(scope) => {
                    if !scopes.contains(&scope_index(scope)) {
                        scopes.push(scope_index(scope));
                    }
                }
                None => out.diagnostics.push(Diagnostic {
                    line,
                    subject: short(selector),
                    reason: Reason::Selector,
                }),
            }
        }
        if scopes.is_empty() {
            continue;
        }
        let body_line = line_of(text, body_start);
        for (prop, value, offset_line) in split_declarations(body, body_line) {
            for &idx in &scopes {
                let scope = [Scope::Base, Scope::Light, Scope::Dark][idx];
                if prop.starts_with("--") {
                    vars[idx].insert(prop.clone(), value.clone());
                }
                decls.push((scope, offset_line, prop.clone(), value.clone()));
            }
        }
    }
}

/// `prop: value` pairs of a declaration block with their line, lower-casing property names.
fn split_declarations(body: &str, first_line: usize) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let (mut depth, mut quote) = (0i32, None::<char>);
    let flush = |from: usize, to: usize, out: &mut Vec<(String, String, usize)>| {
        let piece = &body[from..to];
        if let Some((name, value)) = piece.split_once(':') {
            let name = name.trim();
            if name.is_empty() {
                return;
            }
            let value = value.trim();
            let value = value
                .strip_suffix("!important")
                .map_or(value, str::trim_end);
            let lead = piece.len() - piece.trim_start().len();
            let line = first_line + body[..from + lead].bytes().filter(|b| *b == b'\n').count();
            let name = if name.starts_with("--") {
                name.to_owned()
            } else {
                name.to_ascii_lowercase()
            };
            out.push((name, value.to_owned(), line));
        }
    };
    for (i, c) in body.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth <= 0 => {
                flush(start, i, &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    flush(start, body.len(), &mut out);
    out
}

fn apply_declaration<'a>(
    target: &mut Overrides,
    prop: &str,
    value: &str,
    lookup: &impl Fn(&str) -> Option<&'a str>,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let color_slot: Option<&mut Option<Rgba8>> = match prop {
        "--ls-primary-background-color" | "background-color" => Some(&mut target.background),
        "--ls-secondary-background-color" => Some(&mut target.secondary_background),
        "--ls-tertiary-background-color" => Some(&mut target.tertiary_background),
        "--ls-primary-text-color" | "color" => Some(&mut target.text),
        "--ls-secondary-text-color" => Some(&mut target.secondary_text),
        "--ls-link-text-color" => Some(&mut target.link),
        "--ls-block-bullet-color" => Some(&mut target.bullet),
        "--ls-border-color" => Some(&mut target.border),
        "--ls-selection-background-color" => Some(&mut target.selection),
        _ => None,
    };
    if let Some(slot) = color_slot {
        match parse_color(value, lookup, 0) {
            Some(color) => *slot = Some(color),
            None => diagnostics.push(Diagnostic {
                line,
                subject: format!("{prop}: {}", short(value)),
                reason: Reason::Value,
            }),
        }
        return;
    }
    match prop {
        "font-family" | "--ls-font-family" => match first_family(value, lookup) {
            Some(family) => target.font_family = Some(family),
            None => diagnostics.push(Diagnostic {
                line,
                subject: format!("{prop}: {}", short(value)),
                reason: Reason::Value,
            }),
        },
        "font-size" => match parse_size_px(value) {
            Some(px) => target.font_size = Some(px),
            None => diagnostics.push(Diagnostic {
                line,
                subject: format!("{prop}: {}", short(value)),
                reason: Reason::Value,
            }),
        },
        // Author variables are only inputs of `var()`.
        p if p.starts_with("--") && !p.starts_with("--ls-") => {}
        _ => diagnostics.push(Diagnostic {
            line,
            subject: prop.to_owned(),
            reason: Reason::Property,
        }),
    }
}

/// The first family of a `font-family` list that is not a generic keyword; a list of only
/// generics yields the platform UI font.
fn first_family<'a>(value: &str, lookup: &impl Fn(&str) -> Option<&'a str>) -> Option<String> {
    let value = resolve_var(value, lookup, 0)?;
    let mut generic = false;
    for item in value.split(',') {
        let name = item.trim().trim_matches(['"', '\'']).trim();
        if name.is_empty() {
            continue;
        }
        match name.to_ascii_lowercase().as_str() {
            "serif" | "sans-serif" | "system-ui" | "ui-sans-serif" | "-apple-system"
            | "blinkmacsystemfont" | "monospace" | "ui-monospace" | "cursive" | "fantasy"
            | "inherit" | "initial" | "unset" => generic = true,
            _ => return Some(name.to_owned()),
        }
    }
    generic.then(|| ".SystemUIFont".to_owned())
}

/// Resolves a value that is exactly `var(--name[, fallback])`; other values pass through.
fn resolve_var<'a>(
    value: &str,
    lookup: &impl Fn(&str) -> Option<&'a str>,
    depth: u8,
) -> Option<String> {
    let value = value.trim();
    let Some(inner) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) else {
        return Some(value.to_owned());
    };
    if depth > 4 {
        return None;
    }
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    match lookup(name) {
        Some(found) => resolve_var(found, lookup, depth + 1),
        None => fallback.and_then(|f| resolve_var(f, lookup, depth + 1)),
    }
}

fn parse_size_px(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    let (num, mult) = if let Some(n) = v.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = v.strip_suffix("pt") {
        (n, 4.0 / 3.0)
    } else {
        (
            v.strip_suffix("rem").or_else(|| v.strip_suffix("em"))?,
            16.0,
        )
    };
    let px = num.trim().parse::<f32>().ok()? * mult;
    (px.is_finite() && px > 0.0).then(|| px.clamp(10.0, 40.0))
}

fn parse_color<'a>(
    value: &str,
    lookup: &impl Fn(&str) -> Option<&'a str>,
    depth: u8,
) -> Option<Rgba8> {
    let resolved = resolve_var(value, lookup, depth)?;
    let v = resolved.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(args) = function_args(&v, &["rgba", "rgb"]) {
        let parts = split_args(args);
        if parts.len() < 3 {
            return None;
        }
        let ch = |s: &str| -> Option<u8> {
            if let Some(p) = s.strip_suffix('%') {
                Some((p.trim().parse::<f32>().ok()?.clamp(0.0, 100.0) * 2.55).round() as u8)
            } else {
                Some(s.parse::<f32>().ok()?.clamp(0.0, 255.0).round() as u8)
            }
        };
        let a = parts.get(3).map_or(Some(255), |s| alpha(s))?;
        return Some([ch(&parts[0])?, ch(&parts[1])?, ch(&parts[2])?, a]);
    }
    if let Some(args) = function_args(&v, &["hsla", "hsl"]) {
        let parts = split_args(args);
        if parts.len() < 3 {
            return None;
        }
        let h = parts[0].trim_end_matches("deg").parse::<f32>().ok()?;
        let s = parts[1].strip_suffix('%')?.parse::<f32>().ok()? / 100.0;
        let l = parts[2].strip_suffix('%')?.parse::<f32>().ok()? / 100.0;
        let a = parts.get(3).map_or(Some(255), |s| alpha(s))?;
        let [r, g, b] = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
        return Some([r, g, b, a]);
    }
    named_color(&v)
}

fn alpha(s: &str) -> Option<u8> {
    let f = if let Some(p) = s.strip_suffix('%') {
        p.trim().parse::<f32>().ok()? / 100.0
    } else {
        s.parse::<f32>().ok()?
    };
    Some((f.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn function_args<'a>(v: &'a str, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|n| {
        v.strip_prefix(n)
            .and_then(|r| r.trim_start().strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
    })
}

fn split_args(args: &str) -> Vec<String> {
    args.split([',', '/', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_hex(hex: &str) -> Option<Rgba8> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|n| n * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some([nib(0)?, nib(1)?, nib(2)?, 255]),
        4 => Some([nib(0)?, nib(1)?, nib(2)?, nib(3)?]),
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn named_color(name: &str) -> Option<Rgba8> {
    let rgb = match name {
        "white" => [255, 255, 255],
        "black" => [0, 0, 0],
        "red" => [255, 0, 0],
        "green" => [0, 128, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "orange" => [255, 165, 0],
        "purple" => [128, 0, 128],
        "gray" | "grey" => [128, 128, 128],
        "silver" => [192, 192, 192],
        "teal" => [0, 128, 128],
        "navy" => [0, 0, 128],
        "maroon" => [128, 0, 0],
        "pink" => [255, 192, 203],
        "transparent" => return Some([0, 0, 0, 0]),
        _ => return None,
    };
    Some([rgb[0], rgb[1], rgb[2], 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
/* my theme */
:root {
  --ls-primary-background-color: #fdf6e3;
  --ls-primary-text-color: rgb(88, 110, 117);
  --ls-link-text-color: var(--accent);
  --accent: hsl(205, 70%, 49%);
  --ls-block-bullet-color: #c00;
  --ls-page-title-size: 40px;
}
.dark-theme { --ls-primary-background-color: #002b36; }
body { font-family: "Fira Sans", sans-serif; font-size: 18px }
.cp__sidebar-main-content { max-width: 900px; }
@media (max-width: 600px) { body { font-size: 12px } }
@import url("x.css");
.editor { color: #111 !important; border-radius: 3px }
"#;

    #[test]
    fn maps_documented_subset() {
        let css = parse(SAMPLE);
        assert_eq!(css.base.background, Some([0xfd, 0xf6, 0xe3, 255]));
        assert_eq!(css.base.text, Some([0x11, 0x11, 0x11, 255])); // .editor color wins later
        assert_eq!(css.base.link.map(|c| c[3]), Some(255));
        assert_eq!(css.base.bullet, Some([0xcc, 0, 0, 255]));
        assert_eq!(css.base.font_family.as_deref(), Some("Fira Sans"));
        assert_eq!(css.base.font_size, Some(18.0));
        assert_eq!(css.dark.background, Some([0, 0x2b, 0x36, 255]));
        assert_eq!(css.effective(true).background, css.dark.background);
        assert_eq!(css.effective(false).background, css.base.background);
        assert_eq!(css.effective(true).font_size, Some(18.0));
    }

    #[test]
    fn unsupported_constructs_are_diagnosed_with_lines() {
        let css = parse(SAMPLE);
        let find = |s: &str| css.diagnostics.iter().find(|d| d.subject.contains(s));
        assert_eq!(
            find("--ls-page-title-size").map(|d| d.reason),
            Some(Reason::Property)
        );
        assert_eq!(
            find("cp__sidebar").map(|d| d.reason),
            Some(Reason::Selector)
        );
        assert_eq!(find("@media").map(|d| d.reason), Some(Reason::AtRule));
        assert_eq!(find("@import").map(|d| d.reason), Some(Reason::AtRule));
        assert_eq!(
            find("border-radius").map(|d| d.reason),
            Some(Reason::Property)
        );
        assert_eq!(find("--ls-page-title-size").map(|d| d.line), Some(9));
        // Nothing inside the ignored @media leaked in.
        assert_ne!(css.base.font_size, Some(12.0));
    }

    #[test]
    fn bad_values_and_garbage_never_panic() {
        let css =
            parse("body { --ls-primary-text-color: notacolor; font-size: big } }}} {{ :root {");
        assert!(css.base.text.is_none());
        assert!(css.diagnostics.iter().any(|d| d.reason == Reason::Value));
        assert!(parse("").diagnostics.is_empty());
        let _ = parse("/* unterminated");
        let _ = parse("a { b: \"unterminated");
    }

    #[test]
    fn color_syntaxes() {
        let none = |_: &str| None;
        assert_eq!(parse_color("#abc", &none, 0), Some([0xaa, 0xbb, 0xcc, 255]));
        assert_eq!(
            parse_color("#11223344", &none, 0),
            Some([0x11, 0x22, 0x33, 0x44])
        );
        assert_eq!(
            parse_color("rgba(255, 0, 0, 0.5)", &none, 0),
            Some([255, 0, 0, 128])
        );
        assert_eq!(
            parse_color("hsl(0, 100%, 50%)", &none, 0),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            parse_color("var(--missing, #000)", &none, 0),
            Some([0, 0, 0, 255])
        );
        assert_eq!(parse_color("#12", &none, 0), None);
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size_px("16px"), Some(16.0));
        assert_eq!(parse_size_px("12pt"), Some(16.0));
        assert_eq!(parse_size_px("1rem"), Some(16.0));
        assert_eq!(parse_size_px("90%"), None);
    }
}
