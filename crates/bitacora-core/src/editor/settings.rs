//! Editing preferences that change what commands do (`logseq/config.edn`).

use bitacora_config::{Edn, EffectiveConfig, PreferredWorkflow};

/// Which task markers a cycle goes through (`:preferred-workflow`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum Workflow {
    /// `LATER -> NOW -> DONE` (Logseq's default).
    #[default]
    Now,
    /// `TODO -> DOING -> DONE`.
    Todo,
}

impl Workflow {
    /// The marker a block without one gets.
    #[must_use]
    pub const fn start(self) -> &'static str {
        match self {
            Self::Now => "LATER",
            Self::Todo => "TODO",
        }
    }

    /// The marker after `current` (`None` = no marker); `None` when the cycle ends
    /// (`marker.cljs:40-58`: `TODO>DOING>DONE>none`, `LATER>NOW>DONE>none`; any other marker
    /// restarts the cycle).
    #[must_use]
    pub fn next(self, current: Option<&str>) -> Option<&'static str> {
        match current {
            Some("TODO") => Some("DOING"),
            Some("LATER") => Some("NOW"),
            Some("DOING" | "NOW") => Some("DONE"),
            Some("DONE") => None,
            _ => Some(self.start()),
        }
    }
}

/// Settings read by the planners.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct EditorSettings {
    /// `:editor/logical-outdenting?`: outdenting leaves the following siblings where they are
    /// instead of making them children of the outdented block.
    pub logical_outdenting: bool,
    /// `:preferred-workflow`.
    pub workflow: Workflow,
}

impl EditorSettings {
    /// Reads the settings from the effective graph configuration.
    #[must_use]
    pub fn from_config(cfg: &EffectiveConfig) -> Self {
        Self {
            logical_outdenting: cfg
                .get("editor/logical-outdenting?")
                .and_then(Edn::as_bool)
                .unwrap_or(false),
            workflow: match cfg.preferred_workflow() {
                PreferredWorkflow::Now => Workflow::Now,
                PreferredWorkflow::Todo => Workflow::Todo,
            },
        }
    }
}
