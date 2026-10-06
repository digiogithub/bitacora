//! Read-only block rendering model (BIT-US-0074).
//!
//! Everything here is GPUI-free: it turns block text into [`inline::TextLayout`]s (text plus
//! styled and clickable ranges) and [`model::PageModel`]s (flattened block rows with
//! decorations). The GPUI elements live in `views::page_view`.

pub mod embed;
pub mod highlight;
pub mod inline;
pub mod model;
pub mod query;
pub mod widget;
