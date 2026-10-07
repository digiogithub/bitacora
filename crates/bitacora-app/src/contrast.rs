//! WCAG 2.x contrast ratios for theme colours (BIT-T-0338). Pure functions, no GPUI types: the
//! tests check every bundled theme against AA (4.5:1 for text, 3:1 for the focus ring and caret).

/// An sRGB colour with alpha, channels 0..=255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 = opaque).
    pub a: u8,
}

impl Color {
    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        let digit = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            3 => Some(Self {
                r: digit(0)? * 17,
                g: digit(1)? * 17,
                b: digit(2)? * 17,
                a: 255,
            }),
            6 => Some(Self {
                r: byte(0)?,
                g: byte(2)?,
                b: byte(4)?,
                a: 255,
            }),
            8 => Some(Self {
                r: byte(0)?,
                g: byte(2)?,
                b: byte(4)?,
                a: byte(6)?,
            }),
            _ => None,
        }
    }

    /// This colour drawn over the opaque `under`.
    #[must_use]
    pub fn over(self, under: Self) -> Self {
        let a = f64::from(self.a) / 255.0;
        let mix = |top: u8, bottom: u8| {
            (f64::from(top) * a + f64::from(bottom) * (1.0 - a)).round() as u8
        };
        Self {
            r: mix(self.r, under.r),
            g: mix(self.g, under.g),
            b: mix(self.b, under.b),
            a: 255,
        }
    }

    /// Relative luminance (WCAG 2.x), alpha ignored.
    #[must_use]
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// Contrast ratio of `fg` over `bg` (1.0 to 21.0); a translucent `fg` is composited on `bg`.
#[must_use]
pub fn ratio(fg: Color, bg: Color) -> f64 {
    let fg = fg.over(bg);
    let (a, b) = (fg.luminance(), bg.luminance());
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const BUNDLED: &str = include_str!("../assets/themes/bitacora.json");

    fn c(hex: &str) -> Color {
        Color::parse(hex).expect("colour")
    }

    #[test]
    fn matches_the_wcag_reference_values() {
        assert!((ratio(c("#000"), c("#fff")) - 21.0).abs() < 1e-9);
        assert!((ratio(c("#fff"), c("#fff")) - 1.0).abs() < 1e-9);
        // The classic "#777 on white" example sits just under AA.
        let grey = ratio(c("#777777"), c("#ffffff"));
        assert!((grey - 4.48).abs() < 0.01, "{grey}");
        // Translucent colours are composited first: 50% black on white is #808080.
        let half = ratio(c("#00000080"), c("#ffffff"));
        assert!(
            (half - ratio(c("#7f7f7f"), c("#ffffff"))).abs() < 0.05,
            "{half}"
        );
        assert_eq!(Color::parse("nope"), None);
        assert_eq!(Color::parse("#12"), None);
    }

    /// Pairs that carry text (AA: 4.5:1).
    const TEXT_PAIRS: &[(&str, &str)] = &[
        ("foreground", "background"),
        ("muted.foreground", "background"),
        ("foreground", "muted.background"),
        ("muted.foreground", "muted.background"),
        ("link", "background"),
        ("link", "muted.background"),
        ("primary.foreground", "primary.background"),
        ("secondary.foreground", "secondary.background"),
        ("accent.foreground", "accent.background"),
        ("sidebar.foreground", "sidebar.background"),
        ("muted.foreground", "sidebar.background"),
        ("link", "sidebar.background"),
        ("sidebar.accent.foreground", "sidebar.accent.background"),
        ("popover.foreground", "popover.background"),
        ("tab.foreground", "tab_bar.background"),
        ("tab.active.foreground", "tab.active.background"),
        ("foreground", "list.hover.background"),
        ("foreground", "list.active.background"),
    ];

    /// Pairs that must be visible but are not text (WCAG 2.2 non-text: 3:1).
    const NON_TEXT_PAIRS: &[(&str, &str)] = &[
        ("ring", "background"),
        ("ring", "sidebar.background"),
        ("caret", "background"),
    ];

    fn bundled() -> Vec<(String, BTreeMap<String, String>)> {
        let json: serde_json::Value = serde_json::from_str(BUNDLED).expect("bundled themes");
        json["themes"]
            .as_array()
            .expect("themes")
            .iter()
            .map(|t| {
                let name = t["name"].as_str().expect("name").to_owned();
                let colors = t["colors"]
                    .as_object()
                    .expect("colors")
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                    .collect();
                (name, colors)
            })
            .collect()
    }

    fn check(pairs: &[(&str, &str)], min: f64, failures: &mut Vec<String>) {
        for (theme, colors) in bundled() {
            for (fg, bg) in pairs {
                let (Some(f), Some(b)) = (colors.get(*fg), colors.get(*bg)) else {
                    failures.push(format!("{theme}: missing {fg} or {bg}"));
                    continue;
                };
                let r = ratio(c(f), c(b));
                if r < min {
                    failures.push(format!("{theme}: {fg} {f} on {bg} {b} = {r:.2} (< {min})"));
                }
            }
        }
    }

    #[test]
    fn every_bundled_theme_meets_wcag_aa() {
        let mut failures = Vec::new();
        check(TEXT_PAIRS, 4.5, &mut failures);
        check(NON_TEXT_PAIRS, 3.0, &mut failures);
        assert!(
            failures.is_empty(),
            "contrast failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn selection_keeps_text_readable() {
        // The selection colour is translucent: text over it must stay AA on every theme.
        let mut failures = Vec::new();
        for (theme, colors) in bundled() {
            let bg = c(&colors["background"]);
            let selected = c(&colors["selection.background"]).over(bg);
            let r = ratio(c(&colors["foreground"]), selected);
            if r < 4.5 {
                failures.push(format!("{theme}: text on selection = {r:.2}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
