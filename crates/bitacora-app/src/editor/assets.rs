//! Pasted and dropped files (BIT-US-0096): what comes in from the clipboard or a drop, and the
//! conversion into the core command that saves the files under `assets/` and links them.
//! Naming and links follow Logseq (`bitacora_core::assets`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bitacora_core::assets::{asset_link, asset_path};
use bitacora_core::editor::NewAsset;
use bitacora_core::graph_path::GraphPath;

use crate::ui::text_edit::{ClipboardEntry, ClipboardItem, ImageFormat};

/// Largest file that is attached (it is read into memory and handed to the writer).
pub const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

/// A file waiting to be attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// A file on disk (dropped, or copied in a file manager); read in the background.
    Path(PathBuf),
    /// Bytes that are already in memory (an image on the clipboard).
    Bytes {
        /// File name shown in the link and used for the asset name.
        name: String,
        /// Content.
        bytes: Arc<[u8]>,
    },
}

/// A file ready to be attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// Original file name.
    pub name: String,
    /// Content.
    pub bytes: Arc<[u8]>,
}

/// What the clipboard offers to attach: copied files, or an image. Text (even together with an
/// image) is left to the normal paste.
#[must_use]
pub fn from_clipboard(item: &ClipboardItem) -> Vec<Incoming> {
    let mut out = Vec::new();
    for entry in item.entries() {
        match entry {
            ClipboardEntry::ExternalPaths(paths) => {
                out.extend(paths.paths().iter().cloned().map(Incoming::Path));
            }
            ClipboardEntry::Image(image) => {
                out.push(Incoming::Bytes {
                    name: format!("image.{}", extension_of(image.format)),
                    bytes: Arc::from(image.bytes.as_slice()),
                });
            }
            ClipboardEntry::String(_) => {}
        }
    }
    // Several image flavours of one copy (PNG and BMP) are one picture: keep the first.
    let mut seen_image = false;
    out.retain(|i| match i {
        Incoming::Bytes { .. } => !std::mem::replace(&mut seen_image, true),
        Incoming::Path(_) => true,
    });
    out
}

fn extension_of(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        _ => "png",
    }
}

/// Reads the files (blocking: run it on a background thread). Directories, unreadable files and
/// files over [`MAX_ASSET_BYTES`] are returned in the second list, by name.
#[must_use]
pub fn load(incoming: Vec<Incoming>) -> (Vec<Loaded>, Vec<String>) {
    let mut ok = Vec::new();
    let mut skipped = Vec::new();
    for item in incoming {
        match item {
            Incoming::Bytes { name, bytes } => ok.push(Loaded { name, bytes }),
            Incoming::Path(path) => {
                let name = path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let fits = std::fs::metadata(&path)
                    .is_ok_and(|m| m.is_file() && m.len() <= MAX_ASSET_BYTES);
                match fits.then(|| std::fs::read(&path).ok()).flatten() {
                    Some(bytes) => ok.push(Loaded {
                        name,
                        bytes: Arc::from(bytes),
                    }),
                    None => skipped.push(name),
                }
            }
        }
    }
    (ok, skipped)
}

/// Milliseconds since the Unix epoch (the timestamp in asset names).
#[must_use]
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// The assets (path, content, link) for `files` pasted into the page file `page_file`.
#[must_use]
pub fn plan(files: Vec<Loaded>, page_file: Option<&GraphPath>, epoch_ms: u128) -> Vec<NewAsset> {
    files
        .into_iter()
        .enumerate()
        .filter_map(|(index, f)| {
            let path = asset_path(&f.name, epoch_ms, index).ok()?;
            Some(NewAsset {
                link: asset_link(page_file, &path, &f.name),
                path,
                bytes: f.bytes,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names_and_links_like_logseq() {
        let page = GraphPath::new("pages/sub/x.md").expect("path");
        let files = vec![
            Loaded {
                name: "Screen Shot 2024.png".into(),
                bytes: Arc::from(&[1u8][..]),
            },
            Loaded {
                name: "report 50%.docx".into(),
                bytes: Arc::from(&[2u8][..]),
            },
        ];
        let assets = plan(files, Some(&page), 1_731_580_000_000);
        assert_eq!(
            assets[0].path.as_str(),
            "assets/Screen_Shot_2024_1731580000000_0.png"
        );
        assert_eq!(
            assets[1].link,
            "[report 50%.docx](../../assets/report_50_1731580000000_1.docx)"
        );
    }

    #[test]
    fn load_skips_directories_and_missing_files() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("a b.txt");
        std::fs::write(&file, "hi").expect("write");
        let (ok, skipped) = load(vec![
            Incoming::Path(file),
            Incoming::Path(dir.path().to_path_buf()),
            Incoming::Path(dir.path().join("missing.png")),
        ]);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].name, "a b.txt");
        assert_eq!(skipped.len(), 2);
    }
}
