//! `Palette::dark()` / `light()` must equal `design/tokens/tokens.json` (BIT-SP-0008.R2).

use std::path::Path;

use super::Palette;
use crate::ui::{Hsla, rgb, rgba};

fn colour(hex: &str) -> Hsla {
    let hex = hex.trim_start_matches('#');
    let value = u32::from_str_radix(hex, 16).expect("hex colour");
    match hex.len() {
        6 => rgb(value).into(),
        8 => rgba(value).into(),
        n => panic!("unexpected colour length {n}: {hex}"),
    }
}

fn check(mode: &str, palette: &Palette) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tokens/tokens.json");
    let text = std::fs::read_to_string(&path).expect("read tokens.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse tokens.json");
    let group = json["color"][mode].as_object().expect("colour group");
    let entries = palette.entries();
    assert_eq!(entries.len(), group.len(), "{mode}: token count differs");
    for (name, got) in entries {
        let want = group[name]["$value"].as_str().expect("$value");
        assert_eq!(got, colour(want), "{mode}.{name}");
    }
}

#[test]
fn dark_palette_equals_tokens_json() {
    check("dark", &Palette::dark());
}

#[test]
fn light_palette_equals_tokens_json() {
    check("light", &Palette::light());
}
