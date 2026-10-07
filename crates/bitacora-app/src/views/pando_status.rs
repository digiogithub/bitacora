//! The user-facing Pando connection state (BIT-US-0140, BIT-SP-0009.R4): one enum derived from
//! the machine-local settings, the graph's consent and the live status of the session, shown in
//! the sidebar footer, the top-bar popover and (through `status_chip`) the settings page.
//!
//! Pure data: nothing here touches GPUI, so the derivation is unit-tested exhaustively.

use bitacora_config::PandoSettings;
use bitacora_runtime::PandoStatus;
use rust_i18n::t;

use crate::ui::Hsla;
use crate::ui::theme::BitacoraTheme;
use crate::views::kit::ChipTone;

/// What the user is told about Pando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PandoState {
    /// The user switched Pando off.
    Disabled,
    /// Never set up, or this graph has no consent yet: nothing is contacted.
    NotConfigured,
    /// Starting the server or waiting for its first answer.
    Connecting,
    /// Reachable, authorized and recent enough.
    Ok,
    /// The server rejected the token.
    Unauthorized,
    /// The server does not answer, or the setup is unusable.
    Unreachable,
    /// The server is older than the minimum version.
    TooOld,
}

impl PandoState {
    /// Derives the state. `live` is the session's last status (`None` before the first answer).
    #[must_use]
    pub fn derive(settings: &PandoSettings, consented: bool, live: Option<&PandoStatus>) -> Self {
        if !settings.is_active() {
            return if !settings.enabled && settings.graphs.is_empty() {
                Self::NotConfigured
            } else {
                Self::Disabled
            };
        }
        if !consented {
            return Self::NotConfigured;
        }
        match live {
            None | Some(PandoStatus::Off | PandoStatus::Starting) => Self::Connecting,
            Some(PandoStatus::ConsentRequired) => Self::NotConfigured,
            Some(PandoStatus::Connected { .. }) => Self::Ok,
            Some(PandoStatus::Unauthorized) => Self::Unauthorized,
            Some(PandoStatus::TooOld { .. }) => Self::TooOld,
            Some(PandoStatus::Unavailable { .. }) => Self::Unreachable,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NotConfigured => "not_configured",
            Self::Connecting => "connecting",
            Self::Ok => "ok",
            Self::Unauthorized => "unauthorized",
            Self::Unreachable => "unreachable",
            Self::TooOld => "too_old",
        }
    }

    /// Short label ("Connected").
    #[must_use]
    pub fn label(self) -> String {
        let key = format!("pando_status.state.{}", self.key());
        t!(&key).to_string()
    }

    /// One sentence about the state; `live` supplies the version and the reason.
    #[must_use]
    pub fn detail(self, live: Option<&PandoStatus>) -> String {
        let key = format!("pando_status.detail.{}", self.key());
        match (self, live) {
            (Self::Ok, Some(PandoStatus::Connected { version })) => {
                t!(&key, version = version).to_string()
            }
            (Self::TooOld, Some(PandoStatus::TooOld { version, min })) => {
                t!(&key, version = version, min = min).to_string()
            }
            (Self::Unreachable, Some(PandoStatus::Unavailable { reason }))
                if !reason.is_empty() =>
            {
                format!("{} ({reason})", t!(&key))
            }
            (Self::Ok, _) => t!(&key, version = "").to_string().trim().to_owned(),
            (Self::TooOld, _) => t!(&key, version = "?", min = "").to_string(),
            _ => t!(&key).to_string(),
        }
    }

    /// What stops working while Pando is not [`Ok`](Self::Ok); `None` when nothing is degraded.
    #[must_use]
    pub fn degradation(self) -> Option<String> {
        let key = format!("pando_status.degraded.{}", self.key());
        (self != Self::Ok).then(|| t!(&key).to_string())
    }

    /// Whether Pando features can be used right now.
    #[must_use]
    pub fn is_ok(self) -> bool {
        self == Self::Ok
    }

    /// Whether the user has switched the integration on (anything but off / not set up).
    #[must_use]
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled | Self::NotConfigured)
    }

    /// Whether the state deserves attention (the top-bar dot warns).
    #[must_use]
    pub fn needs_attention(self) -> bool {
        matches!(self, Self::Unauthorized | Self::Unreachable | Self::TooOld)
    }

    /// Chip tone of the state.
    #[must_use]
    pub fn tone(self) -> ChipTone {
        match self {
            Self::Ok => ChipTone::Accent,
            Self::Connecting => ChipTone::Ai,
            Self::Disabled | Self::NotConfigured => ChipTone::Neutral,
            Self::Unauthorized | Self::Unreachable | Self::TooOld => ChipTone::Outline,
        }
    }

    /// Dot colour: `ok`, `warn` or `muted` like the other footer dots.
    #[must_use]
    pub fn dot(self, theme: &BitacoraTheme) -> Hsla {
        match self {
            Self::Ok => theme.colors.ok,
            Self::Connecting => theme.colors.ai,
            Self::Unauthorized | Self::Unreachable | Self::TooOld => theme.colors.warn,
            Self::Disabled | Self::NotConfigured => theme.colors.muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active() -> PandoSettings {
        PandoSettings {
            enabled: true,
            ..PandoSettings::default()
        }
    }

    fn up() -> PandoStatus {
        PandoStatus::Connected {
            version: "1.3.0".into(),
        }
    }

    #[test]
    fn off_and_unconfigured_are_told_apart() {
        let fresh = PandoSettings::default();
        assert_eq!(
            PandoState::derive(&fresh, false, None),
            PandoState::NotConfigured
        );
        let mut was_on = PandoSettings::default();
        was_on.grant_consent("/g", 1);
        assert_eq!(
            PandoState::derive(&was_on, true, None),
            PandoState::Disabled
        );
    }

    #[test]
    fn consent_gates_everything() {
        assert_eq!(
            PandoState::derive(&active(), false, Some(&up())),
            PandoState::NotConfigured
        );
    }

    #[test]
    fn live_status_maps_to_states() {
        let s = active();
        let d = |l: Option<&PandoStatus>| PandoState::derive(&s, true, l);
        assert_eq!(d(None), PandoState::Connecting);
        assert_eq!(d(Some(&PandoStatus::Starting)), PandoState::Connecting);
        assert_eq!(d(Some(&up())), PandoState::Ok);
        assert_eq!(
            d(Some(&PandoStatus::Unauthorized)),
            PandoState::Unauthorized
        );
        assert_eq!(
            d(Some(&PandoStatus::TooOld {
                version: "1.0.0".into(),
                min: "1.2.0".into()
            })),
            PandoState::TooOld
        );
        assert_eq!(
            d(Some(&PandoStatus::Unavailable { reason: "x".into() })),
            PandoState::Unreachable
        );
    }

    #[test]
    fn only_ok_has_no_degradation() {
        assert!(PandoState::Ok.degradation().is_none());
        let msg = PandoState::Unreachable.degradation().unwrap_or_default();
        assert!(msg.contains("keywords") && msg.contains("MCP"), "{msg}");
        assert!(PandoState::Unreachable.needs_attention());
        assert!(!PandoState::Disabled.needs_attention());
        assert!(!PandoState::NotConfigured.is_enabled());
    }

    #[test]
    fn detail_carries_version_and_reason() {
        assert!(PandoState::Ok.detail(Some(&up())).contains("1.3.0"));
        let down = PandoStatus::Unavailable {
            reason: "refused".into(),
        };
        assert!(
            PandoState::Unreachable
                .detail(Some(&down))
                .contains("refused")
        );
    }
}
