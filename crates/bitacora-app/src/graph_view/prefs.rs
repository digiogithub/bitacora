//! Pure helpers of the graph settings panel (BIT-US-0158): slider ranges and steps of the
//! force settings (Logseq's own ranges) and the label search.

use bitacora_config::GraphForce;

/// `(min, max, step)` of a force setting, as in Logseq's sliders.
pub fn force_range(force: GraphForce) -> (i64, i64, i64) {
    match force {
        GraphForce::LinkDist => (10, 180, 10),
        GraphForce::ChargeStrength => (-1000, 1000, 100),
        GraphForce::ChargeRange => (500, 4000, 100),
    }
}

/// `current` moved one step up (`dir > 0`) or down, clamped to the range.
#[allow(clippy::cast_possible_truncation)]
pub fn stepped(force: GraphForce, current: f64, dir: i32) -> i64 {
    let (min, max, step) = force_range(force);
    let now = current.round() as i64;
    let next = if dir > 0 { now + step } else { now - step };
    next.clamp(min, max)
}

/// The label search, normalised (trimmed, lowercase); `None` when empty.
pub fn normalize_query(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    (!q.is_empty()).then_some(q)
}

/// `true` for every name that contains the (normalised) query.
pub fn match_mask<'a>(names: impl IntoIterator<Item = &'a str>, query: &str) -> Vec<bool> {
    names
        .into_iter()
        .map(|n| n.to_lowercase().contains(query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_inside_logseq_ranges() {
        assert_eq!(stepped(GraphForce::LinkDist, 70.0, 1), 80);
        assert_eq!(stepped(GraphForce::LinkDist, 180.0, 1), 180);
        assert_eq!(stepped(GraphForce::LinkDist, 10.0, -1), 10);
        assert_eq!(stepped(GraphForce::ChargeStrength, -600.0, 1), -500);
        assert_eq!(stepped(GraphForce::ChargeStrength, 1000.0, 1), 1000);
        assert_eq!(stepped(GraphForce::ChargeRange, 600.0, -1), 500);
        assert_eq!(stepped(GraphForce::ChargeRange, 500.0, -1), 500);
    }

    #[test]
    fn query_matches_substrings_ignoring_case() {
        assert_eq!(normalize_query("  "), None);
        assert_eq!(normalize_query(" Foo "), Some("foo".to_owned()));
        assert_eq!(
            match_mask(["Alpha", "beta", "ALPHABET"], "alpha"),
            vec![true, false, true]
        );
    }
}
