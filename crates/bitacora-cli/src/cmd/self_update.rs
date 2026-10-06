//! `bitacora-cli self-update`: replace this binary from GitHub Releases (BIT-US-0110).
//!
//! The release workflow attaches `bitacora-cli-<version>-<target>.tar.gz|zip` and a `SHA256SUMS`
//! file to every release. The archive is only installed when its SHA-256 matches the entry in
//! `SHA256SUMS`; any mismatch aborts before the running binary is touched. The binary is replaced
//! atomically (temp file in the same directory, then rename). Installs managed by a package manager
//! or shipped inside the desktop bundle are refused.

use std::cmp::Ordering;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use clap::Args;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// Default GitHub API root of the releases repository.
pub const DEFAULT_API: &str = "https://api.github.com/repos/digio-es/bitacora";
/// Largest download accepted (the CLI archive is a few MiB).
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

/// Options of `self-update`.
#[derive(Debug, Args)]
pub struct SelfUpdateArgs {
    /// Only report whether a newer release exists; do not install it.
    #[arg(long)]
    pub check: bool,
    /// Also consider pre-releases (`-beta.N`, `-rc.N`): the beta update channel.
    #[arg(long)]
    pub prerelease: bool,
    /// Reinstall even when the latest release is not newer than this binary.
    #[arg(long)]
    pub force: bool,
    /// Releases API root (for mirrors and tests).
    #[arg(long, default_value = DEFAULT_API)]
    pub api_url: String,
}

/// What `self-update` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Already on the newest release.
    UpToDate(String),
    /// A newer release exists (only reported with `--check`).
    Available(String),
    /// Binary replaced.
    Updated {
        /// Previous version.
        from: String,
        /// New version.
        to: String,
    },
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Target triple this binary was built for, as used in release asset names.
pub fn target_triple() -> &'static str {
    if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "musl"
    )) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "aarch64",
        target_env = "musl"
    )) {
        "aarch64-unknown-linux-musl"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else {
        "unknown"
    }
}

/// Why a binary location must not be self-updated, if so.
pub fn managed_install(exe: &Path, flatpak: bool) -> Option<&'static str> {
    if flatpak {
        return Some("running inside a Flatpak sandbox; update through Flatpak");
    }
    let p = exe.to_string_lossy().replace('\\', "/");
    if p.contains(".app/Contents/") {
        return Some("part of the Bitacora desktop bundle; update the desktop app instead");
    }
    if p.starts_with("/nix/store/") {
        return Some("installed by Nix; update through Nix");
    }
    if p.contains("/Cellar/") || p.starts_with("/opt/homebrew/bin/") {
        return Some("installed by Homebrew; run `brew upgrade`");
    }
    if p.starts_with("/usr/bin/") || p.starts_with("/usr/lib/") || p.starts_with("/snap/") {
        return Some(
            "installed by the system package manager; update it with your package manager",
        );
    }
    if p.contains("/WindowsApps/") {
        return Some("installed as a packaged Windows app; update through the store or winget");
    }
    None
}

/// Parse `1.2.3` or `1.2.3-beta.4` (a leading `v` is ignored).
fn parse_version(v: &str) -> Option<(Vec<u64>, Option<String>)> {
    let v = v.trim().trim_start_matches('v');
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (v, None),
    };
    let nums = core
        .split('.')
        .map(|n| n.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (nums.len() == 3).then_some((nums, pre))
}

fn cmp_pre(a: &str, b: &str) -> Ordering {
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

/// Semver-style ordering; `None` when either side is not a version.
fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let (an, ap) = parse_version(a)?;
    let (bn, bp) = parse_version(b)?;
    Some(an.cmp(&bn).then_with(|| match (ap, bp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => cmp_pre(&x, &y),
    }))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("bitacora-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> anyhow::Result<Vec<u8>> {
    let mut resp = agent
        .get(url)
        .header(
            "Accept",
            "application/vnd.github+json, application/octet-stream",
        )
        .call()
        .with_context(|| format!("GET {url}"))?;
    resp.body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))
}

/// Look up the expected digest of `file` in a `SHA256SUMS` document.
fn expected_digest(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file).then(|| hash.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Pull the `bitacora-cli[.exe]` binary out of a `.tar.gz` or `.zip` archive.
fn extract_binary(name: &str, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let is_bin = |p: &Path| {
        matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("bitacora-cli" | "bitacora-cli.exe")
        )
    };
    let mut out = Vec::new();
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).context("opening zip")?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).context("reading zip entry")?;
            if f.is_file() && f.enclosed_name().is_some_and(|p| is_bin(&p)) {
                f.read_to_end(&mut out)?;
                return Ok(out);
            }
        }
    } else if name.ends_with(".tar.gz") {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(data));
        for entry in tar.entries().context("opening tar")? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            if entry.header().entry_type().is_file() && is_bin(&path) {
                entry.read_to_end(&mut out)?;
                return Ok(out);
            }
        }
    } else {
        bail!("unsupported archive type: {name}");
    }
    bail!("{name} does not contain a bitacora-cli binary")
}

/// Atomically replace `target` with `new_bytes` (temp file in the same directory, then rename).
fn replace_exe(target: &Path, new_bytes: &[u8]) -> anyhow::Result<()> {
    let dir = target.parent().context("binary has no parent directory")?;
    let tmp = dir.join(format!(".bitacora-cli-update-{}", std::process::id()));
    std::fs::write(&tmp, new_bytes).with_context(|| format!("writing {}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    if let Err(e) = swap(&tmp, target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(unix)]
fn swap(tmp: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(tmp)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(tmp, perms)?;
    std::fs::rename(tmp, target).with_context(|| format!("replacing {}", target.display()))
}

#[cfg(not(unix))]
fn swap(tmp: &Path, target: &Path) -> anyhow::Result<()> {
    // A running executable cannot be overwritten on Windows but it can be renamed.
    let old = target.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(target, &old).context("moving the running binary aside")?;
    if let Err(e) = std::fs::rename(tmp, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e).context("installing the new binary");
    }
    Ok(())
}

/// Check for and (unless `--check`) install the newest release over `exe`.
pub fn update(
    args: &SelfUpdateArgs,
    current: &str,
    exe: &Path,
    target: &str,
) -> anyhow::Result<Outcome> {
    let agent = agent();
    let api = args.api_url.trim_end_matches('/');
    let listing = get_bytes(&agent, &format!("{api}/releases?per_page=30"))?;
    let releases: Vec<Release> =
        serde_json::from_slice(&listing).context("unexpected releases response")?;

    let latest = releases
        .into_iter()
        .filter(|r| !r.draft && (args.prerelease || !r.prerelease))
        .filter_map(|r| {
            let v = r.tag_name.trim_start_matches('v').to_string();
            parse_version(&v)?;
            Some((v, r))
        })
        .max_by(|a, b| compare_versions(&a.0, &b.0).unwrap_or(Ordering::Equal));
    let Some((version, release)) = latest else {
        return Ok(Outcome::UpToDate(current.to_string()));
    };
    let newer = compare_versions(&version, current) == Some(Ordering::Greater);
    if !newer && !args.force {
        return Ok(Outcome::UpToDate(current.to_string()));
    }
    if args.check {
        return Ok(Outcome::Available(version));
    }

    let stem = format!("bitacora-cli-{version}-{target}");
    let archive = release
        .assets
        .iter()
        .find(|a| a.name == format!("{stem}.tar.gz") || a.name == format!("{stem}.zip"))
        .with_context(|| format!("release v{version} has no asset for {target}"))?;
    let sums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .context("release has no SHA256SUMS; refusing to install an unverified binary")?;

    let sums_text = String::from_utf8(get_bytes(&agent, &sums.browser_download_url)?)
        .context("SHA256SUMS is not UTF-8")?;
    let want = expected_digest(&sums_text, &archive.name)
        .with_context(|| format!("SHA256SUMS has no entry for {}", archive.name))?;
    let data = get_bytes(&agent, &archive.browser_download_url)?;
    let got = hex(&Sha256::digest(&data));
    if got != want {
        bail!(
            "checksum mismatch for {}: expected {want}, got {got}",
            archive.name
        );
    }
    let binary = extract_binary(&archive.name, &data)?;
    replace_exe(exe, &binary)?;
    Ok(Outcome::Updated {
        from: current.to_string(),
        to: version,
    })
}

/// Entry point used by `main`.
pub fn run(args: &SelfUpdateArgs) -> anyhow::Result<Outcome> {
    let exe: PathBuf = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .context("cannot locate the running binary")?;
    if let Some(why) = managed_install(&exe, std::env::var_os("FLATPAK_ID").is_some()) {
        bail!("refusing to self-update: {why}");
    }
    update(args, env!("CARGO_PKG_VERSION"), &exe, target_triple())
}

/// Print the outcome.
pub fn print(o: &Outcome) {
    match o {
        Outcome::UpToDate(v) => println!("bitacora-cli {v} is up to date"),
        Outcome::Available(v) => println!("bitacora-cli {v} is available (run `self-update`)"),
        Outcome::Updated { from, to } => println!("bitacora-cli updated {from} -> {to}"),
    }
}

#[cfg(test)]
#[path = "self_update_tests.rs"]
mod tests;
