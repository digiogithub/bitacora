//! `cargo xtask bundle`: build the desktop app and package it with `cargo-packager`
//! (BIT-US-0090).
//!
//! The static packager configuration lives in `crates/bitacora-app/Cargo.toml`
//! (`[package.metadata.packager]`). Signing material is never committed: this command overlays it
//! from environment variables, and only when they are present, so unsigned local bundles work for
//! development.
//!
//! | Variable | Effect |
//! |---|---|
//! | `APPLE_SIGNING_IDENTITY` | macOS: Developer ID identity used to sign the `.app`/`.dmg` |
//! | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | macOS: base64 `.p12` imported into a temporary keychain (read by cargo-packager) |
//! | `APPLE_ID`+`APPLE_PASSWORD`+`APPLE_TEAM_ID` or `APPLE_API_KEY`+`APPLE_API_ISSUER`+`APPLE_API_KEY_PATH` | macOS: notarization credentials (read by cargo-packager) |
//! | `WINDOWS_CERTIFICATE_THUMBPRINT` | Windows: certificate in the store used with `signtool` |
//! | `WINDOWS_SIGN_COMMAND` | Windows: custom sign command (`%1` = file), e.g. Azure Trusted Signing |

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

const APP_PACKAGE: &str = "bitacora-app";

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Merge signing settings from the environment into the packager config.
pub fn apply_signing_overlay(config: &mut Value, env: &dyn Fn(&str) -> Option<String>) {
    let Some(obj) = config.as_object_mut() else {
        return;
    };
    if let Some(identity) = env("APPLE_SIGNING_IDENTITY") {
        let macos = obj.entry("macos").or_insert_with(|| json!({}));
        macos["signingIdentity"] = json!(identity);
    }
    let thumb = env("WINDOWS_CERTIFICATE_THUMBPRINT");
    let cmd = env("WINDOWS_SIGN_COMMAND");
    if thumb.is_some() || cmd.is_some() {
        let win = obj.entry("windows").or_insert_with(|| json!({}));
        win["digestAlgorithm"] = json!("sha256");
        win["timestampUrl"] = json!("http://timestamp.digicert.com");
        win["tsp"] = json!(true);
        if let Some(t) = thumb {
            win["certificateThumbprint"] = json!(t);
        }
        if let Some(c) = cmd {
            win["signCommand"] = json!(c);
        }
    }
}

/// Convert the kebab-case keys of a `[package.metadata.packager]` table to the camelCase the
/// JSON schema of cargo-packager uses.
fn camelize(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| (kebab_to_camel(&k), camelize(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(camelize).collect()),
        other => other,
    }
}

fn kebab_to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '-' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn run_status(cmd: &mut Command) -> anyhow::Result<()> {
    let status = cmd.status().with_context(|| format!("running {cmd:?}"))?;
    if !status.success() {
        bail!("{cmd:?} failed with {status}");
    }
    Ok(())
}

/// Entry point: `cargo xtask bundle [--no-build] [--formats a,b] [--target T]`.
pub fn run(args: &[String]) -> anyhow::Result<bool> {
    let mut build = true;
    let mut formats: Option<String> = None;
    let mut target: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--no-build" => build = false,
            "--formats" => formats = it.next().cloned(),
            "--target" => target = it.next().cloned(),
            other => bail!("unknown bundle option `{other}`"),
        }
    }

    let meta = cargo_metadata::MetadataCommand::new().no_deps().exec()?;
    let pkg = meta
        .packages
        .iter()
        .find(|p| p.name.as_str() == APP_PACKAGE)
        .context("bitacora-app not found in the workspace")?;
    let packager = pkg
        .metadata
        .get("packager")
        .cloned()
        .context("missing [package.metadata.packager] in bitacora-app")?;
    let mut config = camelize(packager);
    config["version"] = json!(pkg.version.to_string());
    if let Some(t) = &target {
        config["targetTriple"] = json!(t);
    }
    if let Some(f) = &formats {
        config["formats"] = json!(f.split(',').map(str::trim).collect::<Vec<_>>());
    }
    apply_signing_overlay(&mut config, &env_nonempty);

    if build {
        let mut cmd = Command::new("cargo");
        cmd.args(["build", "--release", "--locked", "-p", APP_PACKAGE]);
        if let Some(t) = &target {
            cmd.args(["--target", t]);
        }
        run_status(&mut cmd)?;
    }

    let app_dir: PathBuf = pkg
        .manifest_path
        .parent()
        .context("manifest has no parent")?
        .as_std_path()
        .to_path_buf();

    // Passed as a raw JSON string: a config file path would also become the packager's working
    // directory. Relative paths in the metadata are relative to the app crate directory.
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&app_dir)
        .args(["packager", "--release", "-c"])
        .arg(config.to_string());
    run_status(&mut cmd)?;
    let out = app_dir.join("../../target/packager");
    println!("bundles written to {}", display(&out));
    Ok(true)
}

fn display(p: &Path) -> String {
    p.components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camelizes_keys() {
        let v = camelize(
            json!({"product-name": "x", "deb": {"desktop-template": 1}, "a": [{"b-c": 2}]}),
        );
        assert_eq!(
            v,
            json!({"productName": "x", "deb": {"desktopTemplate": 1}, "a": [{"bC": 2}]})
        );
    }

    #[test]
    fn overlay_is_a_noop_without_secrets() {
        let mut c = json!({"productName": "x"});
        apply_signing_overlay(&mut c, &|_| None);
        assert_eq!(c, json!({"productName": "x"}));
    }

    #[test]
    fn overlay_adds_signing_settings() {
        let mut c = json!({"macos": {"minimumSystemVersion": "12.0"}});
        let env = |k: &str| match k {
            "APPLE_SIGNING_IDENTITY" => Some("Developer ID Application: Digio".to_string()),
            "WINDOWS_CERTIFICATE_THUMBPRINT" => Some("ABCDEF".to_string()),
            _ => None,
        };
        apply_signing_overlay(&mut c, &env);
        assert_eq!(c["macos"]["minimumSystemVersion"], "12.0");
        assert_eq!(
            c["macos"]["signingIdentity"],
            "Developer ID Application: Digio"
        );
        assert_eq!(c["windows"]["certificateThumbprint"], "ABCDEF");
        assert_eq!(c["windows"]["digestAlgorithm"], "sha256");
    }
}
