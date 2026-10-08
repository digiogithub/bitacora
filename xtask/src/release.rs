//! Release helpers (BIT-US-0097): tag/version check, release notes from conventional commits,
//! `SHA256SUMS` generation and workspace version bump.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, bail};
use sha2::{Digest as _, Sha256};

/// Release channel implied by a version string.
#[derive(Debug, PartialEq, Eq)]
pub enum Channel {
    /// `X.Y.Z`
    Stable,
    /// `X.Y.Z-beta.N`, `-rc.N`, ...
    Beta,
}

/// Validate a tag `vX.Y.Z[-pre]` against the workspace version and return its channel.
pub fn check_tag(tag: &str, workspace_version: &str) -> anyhow::Result<Channel> {
    let version = tag
        .strip_prefix('v')
        .with_context(|| format!("tag `{tag}` must start with `v`"))?;
    if version != workspace_version {
        bail!("tag `{tag}` does not match the workspace version `{workspace_version}`");
    }
    let core = version.split(['-', '+']).next().unwrap_or(version);
    if core.split('.').count() != 3 || core.split('.').any(|n| n.parse::<u64>().is_err()) {
        bail!("`{version}` is not a MAJOR.MINOR.PATCH version");
    }
    Ok(if version.contains('-') {
        Channel::Beta
    } else {
        Channel::Stable
    })
}

/// One parsed conventional commit subject.
#[derive(Debug, PartialEq, Eq)]
pub struct Commit {
    /// `feat`, `fix`, ...
    pub kind: String,
    /// Optional scope (crate).
    pub scope: Option<String>,
    /// Description after the colon.
    pub text: String,
    /// `!` marker or a `BREAKING CHANGE` footer.
    pub breaking: bool,
}

/// Parse `type(scope)!: text`; `None` for non-conventional subjects and merge commits.
pub fn parse_commit(subject: &str, body: &str) -> Option<Commit> {
    let (head, text) = subject.split_once(": ")?;
    let (head, bang) = match head.strip_suffix('!') {
        Some(h) => (h, true),
        None => (head, false),
    };
    let (kind, scope) = match head.split_once('(') {
        Some((k, rest)) => (k, Some(rest.strip_suffix(')')?.to_string())),
        None => (head, None),
    };
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(Commit {
        kind: kind.to_string(),
        scope,
        text: text.trim().to_string(),
        breaking: bang || body.contains("BREAKING CHANGE"),
    })
}

/// Render release notes markdown, grouped by commit type (chore/test/ci/backlog are omitted).
pub fn render_notes(version: &str, commits: &[Commit]) -> String {
    const SECTIONS: &[(&str, &str)] = &[
        ("feat", "Features"),
        ("fix", "Fixes"),
        ("perf", "Performance"),
        ("refactor", "Refactoring"),
        ("docs", "Documentation"),
    ];
    let mut out = format!("## Bitacora {version}\n\n");
    let breaking: Vec<_> = commits.iter().filter(|c| c.breaking).collect();
    if !breaking.is_empty() {
        out.push_str("### Breaking changes\n\n");
        for c in breaking {
            let _ = writeln!(out, "- {}", line(c));
        }
        out.push('\n');
    }
    let mut any = false;
    for (kind, title) in SECTIONS {
        let items: Vec<_> = commits.iter().filter(|c| c.kind == *kind).collect();
        if items.is_empty() {
            continue;
        }
        any = true;
        let _ = writeln!(out, "### {title}\n");
        for c in items {
            let _ = writeln!(out, "- {}", line(c));
        }
        out.push('\n');
    }
    if !any && out.ends_with("\n\n") && commits.iter().all(|c| !c.breaking) {
        out.push_str("Maintenance release.\n\n");
    }
    out.push_str(
        "### Verify your download\n\nCheck the installer against `SHA256SUMS`; release artifacts \
         also carry build provenance (`gh attestation verify <file> --repo digiogithub/bitacora`).\n",
    );
    out
}

fn line(c: &Commit) -> String {
    match &c.scope {
        Some(s) => format!("**{s}:** {}", c.text),
        None => c.text.clone(),
    }
}

/// Commits in `range` (e.g. `v0.1.0..HEAD`), oldest last, as parsed conventional commits.
fn git_commits(range: &str) -> anyhow::Result<Vec<Commit>> {
    let out = Command::new("git")
        .args(["log", "--no-merges", "--format=%s%x1f%b%x1e", range])
        .output()
        .context("running git log")?;
    if !out.status.success() {
        bail!("git log {range} failed");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split('\x1e')
        .filter_map(|rec| {
            let (s, b) = rec.trim_start_matches('\n').split_once('\x1f')?;
            parse_commit(s.trim(), b)
        })
        .collect())
}

fn latest_tag_before(rev: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0", "--match", "v*", rev])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `cargo xtask release-notes <version> [--to REF] [--from TAG]`.
pub fn notes(args: &[String]) -> anyhow::Result<bool> {
    let version = args.first().context("usage: release-notes <version>")?;
    let mut from: Option<String> = None;
    let mut to = "HEAD".to_string();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from" => from = it.next().cloned(),
            "--to" => to = it.next().cloned().context("--to needs a ref")?,
            other => bail!("unknown option `{other}`"),
        }
    }
    let from = from.or_else(|| latest_tag_before(&format!("{to}^")));
    let range = match from {
        Some(f) => format!("{f}..{to}"),
        None => to,
    };
    print!("{}", render_notes(version, &git_commits(&range)?));
    Ok(true)
}

/// `cargo xtask sha256sums <dir>`: write `<dir>/SHA256SUMS` for every file in `dir`.
pub fn sha256sums(dir: &Path) -> anyhow::Result<bool> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "SHA256SUMS")
        .collect();
    names.sort();
    let mut out = String::new();
    for n in &names {
        let bytes = std::fs::read(dir.join(n))?;
        let _ = writeln!(out, "{}  {n}", hex(&Sha256::digest(&bytes)));
    }
    std::fs::write(dir.join("SHA256SUMS"), out)?;
    println!("SHA256SUMS: {} files", names.len());
    Ok(true)
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn workspace_version() -> anyhow::Result<String> {
    let meta = cargo_metadata::MetadataCommand::new().no_deps().exec()?;
    let pkg = meta
        .packages
        .iter()
        .find(|p| p.name.as_str() == "bitacora-core")
        .context("bitacora-core not found")?;
    Ok(pkg.version.to_string())
}

/// `cargo xtask release-check <tag>`: print `channel=stable|beta`.
pub fn check(tag: &str) -> anyhow::Result<bool> {
    let channel = check_tag(tag, &workspace_version()?)?;
    println!(
        "version={}\nchannel={}",
        tag.trim_start_matches('v'),
        if channel == Channel::Beta {
            "beta"
        } else {
            "stable"
        }
    );
    Ok(true)
}

/// Replace `[workspace.package] version` in the manifest text.
pub fn bump_manifest(text: &str, new: &str) -> anyhow::Result<String> {
    let mut in_pkg = false;
    let mut done = false;
    let mut out = String::new();
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            in_pkg = t == "[workspace.package]";
        }
        if in_pkg && !done && t.starts_with("version") && t.contains('=') {
            let _ = writeln!(out, "version = \"{new}\"");
            done = true;
        } else {
            let _ = writeln!(out, "{l}");
        }
    }
    if !done {
        bail!("no [workspace.package] version found");
    }
    Ok(out)
}

/// `cargo xtask bump <version>`: set the workspace version and refresh `Cargo.lock`.
pub fn bump(new: &str) -> anyhow::Result<bool> {
    check_tag(&format!("v{new}"), new)?;
    let path = Path::new("Cargo.toml");
    let text = std::fs::read_to_string(path)?;
    std::fs::write(path, bump_manifest(&text, new)?)?;
    let status = Command::new("cargo")
        .args(["update", "--workspace", "--offline"])
        .status()
        .context("cargo update")?;
    if !status.success() {
        bail!("cargo update --workspace failed");
    }
    println!("workspace version is now {new}; commit, then tag v{new}");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_must_match_workspace_version() {
        assert_eq!(check_tag("v1.2.3", "1.2.3").ok(), Some(Channel::Stable));
        assert_eq!(
            check_tag("v1.2.3-beta.1", "1.2.3-beta.1").ok(),
            Some(Channel::Beta)
        );
        assert!(check_tag("v1.2.4", "1.2.3").is_err());
        assert!(check_tag("1.2.3", "1.2.3").is_err());
        assert!(check_tag("v1.2", "1.2").is_err());
    }

    #[test]
    fn parses_conventional_commits() {
        let c = parse_commit("feat(core)!: drop old op", "").expect("parse");
        assert_eq!(c.kind, "feat");
        assert_eq!(c.scope.as_deref(), Some("core"));
        assert!(c.breaking);
        assert!(parse_commit("Merge branch 'x'", "").is_none());
        assert!(
            parse_commit("fix: a", "BREAKING CHANGE: b")
                .expect("parse")
                .breaking
        );
    }

    #[test]
    fn renders_grouped_notes() {
        let commits = vec![
            parse_commit("feat(app): bundles", "").expect("c"),
            parse_commit("fix: crash", "").expect("c"),
            parse_commit("chore: noise", "").expect("c"),
        ];
        let md = render_notes("1.0.0", &commits);
        assert!(md.contains("### Features\n\n- **app:** bundles"));
        assert!(md.contains("### Fixes\n\n- crash"));
        assert!(!md.contains("noise"));
    }

    #[test]
    fn bumps_only_the_workspace_version() {
        let t = "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nversion = \"9\"\n";
        let b = bump_manifest(t, "0.2.0").expect("bump");
        assert!(b.contains("version = \"0.2.0\""));
        assert!(b.contains("version = \"9\""));
    }
}
