//! Task syntax: markers and priorities ([`head`]), org timestamps with `SCHEDULED:` / `DEADLINE:`
//! planning lines ([`timestamp`]) and drawers with `:LOGBOOK:` clock entries ([`drawer`]).
//!
//! Like everything in this crate these are read-only analyses with byte spans; a writer that
//! changes a marker or a clock line builds the new text itself (see the `format_*` helpers) and
//! splices it into the original bytes.

pub mod drawer;
pub mod head;
pub mod timestamp;

pub use drawer::{
    Clock, Drawer, LogEntry, elapsed_seconds, find_drawers, format_clock, format_duration,
    parse_clock, parse_duration, parse_logbook, total_seconds,
};
pub use head::{BlockHead, Marker, parse_head};
pub use timestamp::{
    Date, Planning, PlanningKind, RepeatKind, RepeatUnit, Repeater, Time, Timestamp,
    parse_planning_line, parse_timestamp,
};
