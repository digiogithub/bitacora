//! Layer 2: property scanning (`key:: value` lines, `#+key: value` directives and Markdown
//! `:PROPERTIES:` drawers), key normalisation and value interpretation.
//!
//! Everything is read-only: the scanners return byte spans into the original input and never
//! rewrite anything (`serialize(parse(bytes)) == bytes` is untouched).

pub mod drawer;
pub mod scan;
pub mod value;

pub use scan::{
    GroupOrigin, PropLine, PropLineKind, PropertyGroup, PropertyScan, is_valid_key, normalize_key,
    scan_properties,
};
pub use value::{PropValue, PropertyConfig, interpret, scan_refs};
