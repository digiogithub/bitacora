//! File-level policies that are not page merges (BIT-SP-0006.R17): line diff3 for text files,
//! keep-both for whiteboards, conflict copies for binaries, naming of conflict copies.

use bitacora_merge::merge_lines;

use super::markers::has_marker_line;

/// Result of a line merge of a plain-text file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMerge {
    /// Merged text in ours' line ending and BOM; never contains conflict markers.
    pub text: String,
    /// Whether overlapping edits remain (the text then holds ours).
    pub conflict: bool,
}

fn split_bom(s: &str) -> (bool, &str) {
    match s.strip_prefix('\u{feff}') {
        Some(r) => (true, r),
        None => (false, s),
    }
}

/// Line diff3 of base/ours/theirs. BOM and CRLF are normalised for the comparison and restored
/// from ours. On overlap the result is ours unchanged.
pub fn merge_text_lines(base: &str, ours: &str, theirs: &str) -> TextMerge {
    let (bom, o_body) = split_bom(ours);
    let crlf = o_body.contains("\r\n");
    let norm = |s: &str| split_bom(s).1.replace("\r\n", "\n");
    let (b, o, t) = (norm(base), norm(ours), norm(theirs));
    let r = merge_lines(&b, &o, &t);
    if !r.conflicts.is_empty() || (has_marker_line(&r.output) && !has_marker_line(&o)) {
        return TextMerge {
            text: ours.to_owned(),
            conflict: true,
        };
    }
    let mut text = if crlf {
        r.output.replace('\n', "\r\n")
    } else {
        r.output
    };
    if bom {
        text.insert(0, '\u{feff}');
    }
    TextMerge {
        text,
        conflict: false,
    }
}

/// `dir/stem (conflict-<tag>).ext`, used for assets and as a fallback for whiteboards.
pub fn conflict_copy_path(path: &str, tag: &str) -> String {
    copy_path(path, &format!("conflict-{tag}"))
}

/// `dir/stem (conflict <device> <yyyy-mm-dd>).ext`, used for whiteboards and drawings.
pub fn device_copy_path(path: &str, device: &str, date: &str) -> String {
    let device: String = device
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    copy_path(path, &format!("conflict {device} {date}"))
}

fn copy_path(path: &str, label: &str) -> String {
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, path),
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, Some(e)),
        _ => (name, None),
    };
    let mut out = String::new();
    if let Some(d) = dir {
        out.push_str(d);
        out.push('/');
    }
    out.push_str(&format!("{stem} ({label})"));
    if let Some(e) = ext {
        out.push('.');
        out.push_str(e);
    }
    out
}

/// `yyyy-mm-dd` (UTC) of a Unix timestamp.
pub fn iso_date(epoch_secs: i64) -> String {
    // Civil-from-days (proleptic Gregorian), after H. Hinnant's public-domain description.
    let days = epoch_secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_names() {
        assert_eq!(
            conflict_copy_path("assets/diagram_1696000000.png", "c3d4e5f"),
            "assets/diagram_1696000000 (conflict-c3d4e5f).png"
        );
        assert_eq!(conflict_copy_path("noext", "x"), "noext (conflict-x)");
        assert_eq!(
            conflict_copy_path(".gitignore", "x"),
            ".gitignore (conflict-x)"
        );
        assert_eq!(
            device_copy_path("whiteboards/w.edn", "phone jose/1", "2026-10-06"),
            "whiteboards/w (conflict phone-jose-1 2026-10-06).edn"
        );
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_791_244_800), "2026-10-06");
        assert_eq!(iso_date(951_782_400), "2000-02-29");
        assert_eq!(iso_date(-1), "1969-12-31");
    }

    #[test]
    fn line_merge_clean_conflict_crlf_and_bom() {
        let m = merge_text_lines("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        assert_eq!((m.text.as_str(), m.conflict), ("A\nb\nC\n", false));
        let m = merge_text_lines("a\n", "b\n", "c\n");
        assert_eq!((m.text.as_str(), m.conflict), ("b\n", true));
        // Ours uses CRLF and a BOM, theirs LF: no spurious conflict, ours' style is kept.
        let m = merge_text_lines("a\nb\n", "\u{feff}A\r\nb\r\n", "a\nB\n");
        assert_eq!((m.text.as_str(), m.conflict), ("\u{feff}A\r\nB\r\n", false));
    }
}
