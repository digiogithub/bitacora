//! Graph view settings (`:graph/settings` and `:graph/forcesettings`), Logseq 0.10.x keys.
//!
//! Reading is tolerant: a missing key or a value of the wrong type falls back to the default, so
//! a hand-edited config never breaks the view. Writing goes through [`ConfigEditor`], which
//! splices single values and leaves every other byte of the file untouched.

use crate::config::EffectiveConfig;
use crate::edit::ConfigEditor;
use crate::edn::Edn;
use crate::error::Error;

/// `:graph/settings` map key.
const SETTINGS: &str = "graph/settings";
/// `:graph/forcesettings` map key.
const FORCES: &str = "graph/forcesettings";

/// Default `:link-dist`.
pub const DEFAULT_LINK_DIST: f64 = 70.0;
/// Default `:charge-strength`.
pub const DEFAULT_CHARGE_STRENGTH: f64 = -600.0;
/// Default `:charge-range`.
pub const DEFAULT_CHARGE_RANGE: f64 = 600.0;

/// A boolean key of `:graph/settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphToggle {
    /// `:journal?` (default off).
    Journals,
    /// `:orphan-pages?` (default on).
    OrphanPages,
    /// `:builtin-pages?` (default off).
    BuiltinPages,
    /// `:excluded-pages?`: show pages that opt out with `exclude-from-graph-view:: true`
    /// (default off).
    ExcludedPages,
}

impl GraphToggle {
    /// Every toggle.
    pub const ALL: [GraphToggle; 4] = [
        Self::Journals,
        Self::OrphanPages,
        Self::BuiltinPages,
        Self::ExcludedPages,
    ];

    /// The keyword name (without `:`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Journals => "journal?",
            Self::OrphanPages => "orphan-pages?",
            Self::BuiltinPages => "builtin-pages?",
            Self::ExcludedPages => "excluded-pages?",
        }
    }

    /// The value when the key is absent.
    pub fn default_value(self) -> bool {
        matches!(self, Self::OrphanPages)
    }
}

/// A numeric key of `:graph/forcesettings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphForce {
    /// `:link-dist`.
    LinkDist,
    /// `:charge-strength`.
    ChargeStrength,
    /// `:charge-range`.
    ChargeRange,
}

impl GraphForce {
    /// Every force key.
    pub const ALL: [GraphForce; 3] = [Self::LinkDist, Self::ChargeStrength, Self::ChargeRange];

    /// The keyword name (without `:`).
    pub fn key(self) -> &'static str {
        match self {
            Self::LinkDist => "link-dist",
            Self::ChargeStrength => "charge-strength",
            Self::ChargeRange => "charge-range",
        }
    }

    /// The value when the key is absent (what the "reset" restores).
    pub fn default_value(self) -> f64 {
        match self {
            Self::LinkDist => DEFAULT_LINK_DIST,
            Self::ChargeStrength => DEFAULT_CHARGE_STRENGTH,
            Self::ChargeRange => DEFAULT_CHARGE_RANGE,
        }
    }
}

/// The graph view settings of a config.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphViewSettings {
    /// `:journal?`.
    pub journals: bool,
    /// `:orphan-pages?`.
    pub orphan_pages: bool,
    /// `:builtin-pages?`.
    pub builtin_pages: bool,
    /// `:excluded-pages?`.
    pub excluded_pages: bool,
    /// `:link-dist`.
    pub link_dist: f64,
    /// `:charge-strength`.
    pub charge_strength: f64,
    /// `:charge-range`.
    pub charge_range: f64,
}

impl Default for GraphViewSettings {
    fn default() -> Self {
        Self {
            journals: GraphToggle::Journals.default_value(),
            orphan_pages: GraphToggle::OrphanPages.default_value(),
            builtin_pages: GraphToggle::BuiltinPages.default_value(),
            excluded_pages: GraphToggle::ExcludedPages.default_value(),
            link_dist: DEFAULT_LINK_DIST,
            charge_strength: DEFAULT_CHARGE_STRENGTH,
            charge_range: DEFAULT_CHARGE_RANGE,
        }
    }
}

impl GraphViewSettings {
    /// The value of a toggle.
    pub fn toggle(&self, toggle: GraphToggle) -> bool {
        match toggle {
            GraphToggle::Journals => self.journals,
            GraphToggle::OrphanPages => self.orphan_pages,
            GraphToggle::BuiltinPages => self.builtin_pages,
            GraphToggle::ExcludedPages => self.excluded_pages,
        }
    }

    /// Sets a toggle.
    pub fn set_toggle(&mut self, toggle: GraphToggle, value: bool) {
        match toggle {
            GraphToggle::Journals => self.journals = value,
            GraphToggle::OrphanPages => self.orphan_pages = value,
            GraphToggle::BuiltinPages => self.builtin_pages = value,
            GraphToggle::ExcludedPages => self.excluded_pages = value,
        }
    }

    /// The value of a force.
    pub fn force(&self, force: GraphForce) -> f64 {
        match force {
            GraphForce::LinkDist => self.link_dist,
            GraphForce::ChargeStrength => self.charge_strength,
            GraphForce::ChargeRange => self.charge_range,
        }
    }

    /// Sets a force.
    pub fn set_force(&mut self, force: GraphForce, value: f64) {
        match force {
            GraphForce::LinkDist => self.link_dist = value,
            GraphForce::ChargeStrength => self.charge_strength = value,
            GraphForce::ChargeRange => self.charge_range = value,
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn number(e: &Edn) -> Option<f64> {
    match e {
        Edn::Int(i) => Some(*i as f64),
        Edn::Float(f) if f.is_finite() => Some(*f),
        _ => None,
    }
}

impl EffectiveConfig {
    /// `:graph/settings` and `:graph/forcesettings` with Logseq's defaults for absent keys.
    pub fn graph_view_settings(&self) -> GraphViewSettings {
        let mut out = GraphViewSettings::default();
        let settings = self.get(SETTINGS);
        for t in GraphToggle::ALL {
            if let Some(v) = settings.and_then(|s| s.get(t.key())).and_then(Edn::as_bool) {
                out.set_toggle(t, v);
            }
        }
        let forces = self.get(FORCES);
        for f in GraphForce::ALL {
            if let Some(v) = forces.and_then(|s| s.get(f.key())).and_then(number) {
                out.set_force(f, v);
            }
        }
        out
    }
}

impl ConfigEditor {
    /// Sets one key of `:graph/settings`, creating the map when missing.
    ///
    /// # Errors
    /// [`Error`] when the config text cannot be edited.
    pub fn set_graph_toggle(&mut self, toggle: GraphToggle, value: bool) -> Result<(), Error> {
        self.assoc(&[SETTINGS, toggle.key()], &Edn::Bool(value))
    }

    /// Sets one key of `:graph/forcesettings` (whole numbers are written as integers).
    ///
    /// # Errors
    /// [`Error`] when the config text cannot be edited.
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_graph_force(&mut self, force: GraphForce, value: f64) -> Result<(), Error> {
        let edn = if value.fract() == 0.0 && value.abs() < 1e15 {
            Edn::Int(value as i64)
        } else {
            Edn::Float(value)
        };
        self.assoc(&[FORCES, force.key()], &edn)
    }

    /// Removes `:graph/forcesettings` (the "reset" of the Forces section); other keys stay.
    ///
    /// # Errors
    /// [`Error`] when the config text cannot be edited.
    pub fn reset_graph_forces(&mut self) -> Result<bool, Error> {
        self.dissoc(&[FORCES])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "{;; my graph\n :meta/version 1 ;; keep\n\n ;; Graph view configuration.\n ;; :graph/settings\n ;; {:journal? false}\n :favorites [\"A\" \"B\"] ; fav\n}\n";

    fn read(text: &str) -> GraphViewSettings {
        EffectiveConfig::from_texts(None, Some(text)).graph_view_settings()
    }

    #[test]
    fn defaults_follow_logseq() {
        let s = read(SRC);
        assert_eq!(s, GraphViewSettings::default());
        assert!(s.orphan_pages && !s.journals && !s.builtin_pages && !s.excluded_pages);
        assert_eq!(
            (s.link_dist, s.charge_strength, s.charge_range),
            (70.0, -600.0, 600.0)
        );
    }

    #[test]
    fn reads_logseq_shaped_values_and_ignores_bad_types() {
        let s = read(
            "{:graph/settings {:journal? true :orphan-pages? false :builtin-pages? \"x\"}\n :graph/forcesettings {:link-dist 180 :charge-strength -250.5 :charge-range \"far\"}}",
        );
        assert!(s.journals && !s.orphan_pages && !s.builtin_pages);
        assert_eq!(s.link_dist, 180.0);
        assert_eq!(s.charge_strength, -250.5);
        assert_eq!(s.charge_range, 600.0);
    }

    #[test]
    fn new_entries_keep_every_other_byte() {
        let mut ed = ConfigEditor::parse(SRC).expect("parse");
        ed.set_graph_toggle(GraphToggle::Journals, true).expect("t");
        ed.set_graph_force(GraphForce::LinkDist, 180.0).expect("f");
        let out = ed.into_text();
        for kept in [
            ";; my graph",
            ":meta/version 1 ;; keep",
            ";; {:journal? false}",
            ":favorites [\"A\" \"B\"] ; fav",
        ] {
            assert!(out.contains(kept), "{kept} lost in {out}");
        }
        let s = read(&out);
        assert!(s.journals);
        assert_eq!(s.link_dist, 180.0);
        assert!(s.orphan_pages);
    }

    #[test]
    fn existing_values_are_replaced_in_place() {
        let src = "{:graph/settings {:journal? false ;; why\n :orphan-pages? true}\n :graph/forcesettings {:link-dist 100}}\n";
        let mut ed = ConfigEditor::parse(src).expect("parse");
        ed.set_graph_toggle(GraphToggle::Journals, true).expect("t");
        ed.set_graph_force(GraphForce::LinkDist, 140.0).expect("f");
        assert_eq!(
            ed.into_text(),
            "{:graph/settings {:journal? true ;; why\n :orphan-pages? true}\n :graph/forcesettings {:link-dist 140}}\n"
        );
    }

    #[test]
    fn reset_removes_only_the_force_map() {
        let src = "{:graph/settings {:journal? true}\n :graph/forcesettings {:link-dist 100}\n :favorites []}\n";
        let mut ed = ConfigEditor::parse(src).expect("parse");
        assert!(ed.reset_graph_forces().expect("reset"));
        let out = ed.into_text();
        assert!(out.contains(":graph/settings {:journal? true}"));
        assert!(out.contains(":favorites []"));
        assert!(!out.contains("forcesettings"));
        assert_eq!(read(&out).link_dist, 70.0);
    }

    #[test]
    fn an_empty_map_gets_the_entry() {
        let mut ed = ConfigEditor::parse("{}\n").expect("parse");
        ed.set_graph_toggle(GraphToggle::OrphanPages, false)
            .expect("t");
        assert!(!read(&ed.into_text()).orphan_pages);
    }
}
