//! `cargo xtask check-deps`: dependency-direction, GPUI pin and tokio checks.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, MetadataCommand};

/// One workspace member with its declared dependencies.
#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub is_member: bool,
    /// Normal and build dependencies actually resolved (name only).
    pub deps: BTreeSet<String>,
    /// Declared (normal/build) dependencies: (name, version requirement).
    pub declared: Vec<(String, String)>,
}

/// Allowed edges between workspace crates (normal dependencies only).
fn allowed_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let set = |xs: &[&'static str]| xs.iter().copied().collect::<BTreeSet<_>>();
    let frontends = set(&[
        "bitacora-markdown",
        "bitacora-config",
        "bitacora-merge",
        "bitacora-core",
        "bitacora-watch",
        "bitacora-index",
        "bitacora-sync",
        "bitacora-mcp",
    ]);
    // ADR-034: `bitacora-graph` is a leaf layout engine; only the runtime and app may use it.
    let with_graph = |base: &BTreeSet<&'static str>| {
        let mut s = base.clone();
        s.insert("bitacora-graph");
        s
    };
    // ADR-028: `bitacora-pando` sits between index and runtime; the runtime and the front ends
    // may name it (events, settings types).
    let with_pando = |base: &BTreeSet<&'static str>| {
        let mut s = base.clone();
        s.insert("bitacora-pando");
        s
    };
    let with_runtime = |base: &BTreeSet<&'static str>| {
        let mut s = base.clone();
        s.insert("bitacora-runtime");
        s
    };
    BTreeMap::from([
        ("bitacora-markdown", set(&[])),
        ("bitacora-config", set(&[])),
        ("bitacora-merge", set(&["bitacora-markdown"])),
        (
            "bitacora-core",
            set(&["bitacora-markdown", "bitacora-merge", "bitacora-config"]),
        ),
        ("bitacora-watch", set(&["bitacora-core"])),
        (
            "bitacora-index",
            set(&["bitacora-core", "bitacora-config", "bitacora-markdown"]),
        ),
        (
            "bitacora-sync",
            set(&["bitacora-core", "bitacora-merge", "bitacora-config"]),
        ),
        (
            "bitacora-mcp",
            set(&[
                "bitacora-core",
                "bitacora-index",
                "bitacora-sync",
                "bitacora-config",
                "bitacora-markdown",
            ]),
        ),
        // ADR-024: the headless session composing core+index+watch+sync+mcp; no UI.
        ("bitacora-runtime", with_pando(&with_graph(&frontends))),
        ("bitacora-graph", set(&[])),
        // ADR-027: the generic SDK depends on no bitacora crate.
        ("pando-rs", set(&[])),
        // ADR-028: index <- pando <- runtime.
        (
            "bitacora-pando",
            set(&[
                "bitacora-config",
                "bitacora-core",
                "bitacora-index",
                "pando-rs",
            ]),
        ),
        (
            "bitacora-app",
            with_pando(&with_graph(&with_runtime(&frontends))),
        ),
        ("bitacora-cli", with_pando(&with_runtime(&frontends))),
        // Dev-only helpers: nothing may depend on it in [dependencies].
        ("bitacora-testkit", set(&[])),
        ("xtask", set(&[])),
    ])
}

/// Run all checks against the real workspace. Returns `Ok(true)` when clean.
pub fn run() -> Result<bool> {
    let metadata = MetadataCommand::new()
        .exec()
        .context("running `cargo metadata`")?;
    let resolve = metadata.resolve.context("metadata has no resolve graph")?;
    let ids: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|p| (p.id.clone(), p))
        .collect();
    let members: BTreeSet<_> = metadata.workspace_members.iter().cloned().collect();

    let mut pkgs = Vec::new();
    for node in &resolve.nodes {
        let Some(pkg) = ids.get(&node.id) else {
            continue;
        };
        let deps = node
            .deps
            .iter()
            .filter(|d| {
                d.dep_kinds
                    .iter()
                    .any(|k| k.kind != DependencyKind::Development)
            })
            .filter_map(|d| ids.get(&d.pkg).map(|p| p.name.to_string()))
            .collect();
        let declared = pkg
            .dependencies
            .iter()
            .filter(|d| d.kind != DependencyKind::Development)
            .map(|d| (d.name.clone(), d.req.to_string()))
            .collect();
        pkgs.push(Pkg {
            name: pkg.name.to_string(),
            is_member: members.contains(&node.id),
            deps,
            declared,
        });
    }

    let errors = check(&pkgs);
    if errors.is_empty() {
        let n = pkgs.iter().filter(|p| p.is_member).count();
        println!(
            "check-deps: OK ({n} workspace crates, dependency direction, GPUI pin, no tokio in core)"
        );
        Ok(true)
    } else {
        for e in &errors {
            eprintln!("check-deps: ERROR: {e}");
        }
        eprintln!("check-deps: {} problem(s) found", errors.len());
        Ok(false)
    }
}

/// Pure check over a package graph; returns human-readable violations.
pub fn check(pkgs: &[Pkg]) -> Vec<String> {
    let mut errors = Vec::new();
    let by_name: BTreeMap<&str, &Pkg> = pkgs.iter().map(|p| (p.name.as_str(), p)).collect();
    let allowed = allowed_edges();
    let is_gpui = |n: &str| n == "gpui" || n.starts_with("gpui-") || n.starts_with("gpui_");

    // Transitive closure of normal dependencies.
    let closure = |start: &str| -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start.to_string()];
        while let Some(n) = stack.pop() {
            if let Some(p) = by_name.get(n.as_str()) {
                for d in &p.deps {
                    if seen.insert(d.clone()) {
                        stack.push(d.clone());
                    }
                }
            }
        }
        seen
    };

    for p in pkgs.iter().filter(|p| p.is_member) {
        // 1. only bitacora-app may reach GPUI.
        if p.name != "bitacora-app"
            && let Some(g) = closure(&p.name).iter().find(|d| is_gpui(d))
        {
            errors.push(format!(
                "`{}` depends (directly or transitively) on `{g}`; only `bitacora-app` may use gpui-kit (ADR-001)",
                p.name
            ));
        }
        // 2. no direct `gpui` dependency anywhere; gpui-kit must be pinned with `=`.
        for (dep, req) in &p.declared {
            if dep == "gpui" {
                errors.push(format!(
                    "`{}` declares a direct `gpui` dependency; use `gpui_kit::gpui` (ADR-001)",
                    p.name
                ));
            }
            if dep == "gpui-kit" && !req.trim_start().starts_with('=') {
                errors.push(format!(
                    "`{}` requires gpui-kit `{req}`; it must be an exact `=x.y.z` pin (ADR-001)",
                    p.name
                ));
            }
        }
        // 3. dependency direction among workspace crates.
        match allowed.get(p.name.as_str()) {
            None => errors.push(format!(
                "workspace crate `{}` is not known to xtask check-deps; add it to the allowed-edge table",
                p.name
            )),
            Some(ok) => {
                for d in &p.deps {
                    let workspace_dep =
                        by_name.get(d.as_str()).is_some_and(|q| q.is_member);
                    if workspace_dep && !ok.contains(d.as_str()) {
                        errors.push(format!(
                            "forbidden dependency edge `{}` -> `{d}` (see the dependency direction in AGENTS.md section 4)",
                            p.name
                        ));
                    }
                }
            }
        }
    }
    // `bitacora-merge` may only use `bitacora-markdown` (covered by the table, kept explicit for ADR-016).
    // 4. core is synchronous: no tokio in its closure.
    if by_name.contains_key("bitacora-core") && closure("bitacora-core").contains("tokio") {
        errors
            .push("`bitacora-core` has `tokio` in its normal dependency closure (ADR-012)".into());
    }
    // 5. ADR-028: the Pando SDK and integration crate never enter the core closure.
    if by_name.contains_key("bitacora-core") {
        let c = closure("bitacora-core");
        for banned in ["pando-rs", "bitacora-pando"] {
            if c.contains(banned) {
                errors.push(format!(
                    "`bitacora-core` has `{banned}` in its normal dependency closure (ADR-028)"
                ));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, member: bool, deps: &[&str]) -> Pkg {
        Pkg {
            name: name.into(),
            is_member: member,
            deps: deps.iter().map(|s| s.to_string()).collect(),
            declared: deps.iter().map(|s| (s.to_string(), "*".into())).collect(),
        }
    }

    fn good() -> Vec<Pkg> {
        let mut kit = pkg("gpui-kit", false, &["gpui-pre"]);
        kit.declared = vec![];
        vec![
            pkg("bitacora-markdown", true, &[]),
            pkg("bitacora-merge", true, &["bitacora-markdown"]),
            pkg("bitacora-config", true, &[]),
            pkg(
                "bitacora-core",
                true,
                &["bitacora-markdown", "bitacora-merge", "bitacora-config"],
            ),
            pkg("bitacora-app", true, &["bitacora-core", "gpui-kit"]),
            kit,
            pkg("gpui-pre", false, &[]),
        ]
    }

    fn with_app_pin(mut v: Vec<Pkg>, req: &str) -> Vec<Pkg> {
        for p in &mut v {
            if p.name == "bitacora-app" {
                p.declared = vec![("gpui-kit".into(), req.into())];
            }
        }
        v
    }

    #[test]
    fn clean_graph_passes() {
        assert_eq!(check(&with_app_pin(good(), "=0.7.1")), Vec::<String>::new());
    }

    #[test]
    fn gpui_kit_in_core_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        for p in &mut v {
            if p.name == "bitacora-core" {
                p.deps.insert("gpui-kit".into());
            }
        }
        let errs = check(&v);
        assert!(
            errs.iter().any(|e| e.contains("only `bitacora-app`")),
            "{errs:?}"
        );
    }

    #[test]
    fn app_edge_into_core_reverse_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        for p in &mut v {
            if p.name == "bitacora-core" {
                p.deps.insert("bitacora-app".into());
            }
        }
        let errs = check(&v);
        assert!(
            errs.iter()
                .any(|e| e.contains("`bitacora-core` -> `bitacora-app`")),
            "{errs:?}"
        );
    }

    #[test]
    fn merge_depending_on_core_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        for p in &mut v {
            if p.name == "bitacora-merge" {
                p.deps.insert("bitacora-core".into());
            }
        }
        assert!(!check(&v).is_empty());
    }

    #[test]
    fn unpinned_gpui_kit_fails() {
        let errs = check(&with_app_pin(good(), "^0.7.1"));
        assert!(errs.iter().any(|e| e.contains("exact")), "{errs:?}");
    }

    #[test]
    fn direct_gpui_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        for p in &mut v {
            if p.name == "bitacora-app" {
                p.declared.push(("gpui".into(), "0.2".into()));
            }
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("direct `gpui`")), "{errs:?}");
    }

    #[test]
    fn tokio_in_core_closure_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        v.push(pkg("tokio", false, &[]));
        for p in &mut v {
            if p.name == "bitacora-config" {
                p.deps.insert("tokio".into());
            }
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("tokio")), "{errs:?}");
    }

    #[test]
    fn pando_in_core_closure_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        v.push(pkg("pando-rs", true, &[]));
        for p in &mut v {
            if p.name == "bitacora-config" {
                p.deps.insert("pando-rs".into());
            }
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("ADR-028")), "{errs:?}");
    }

    #[test]
    fn pando_sdk_depending_on_bitacora_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        v.push(pkg("pando-rs", true, &["bitacora-config"]));
        assert!(!check(&v).is_empty());
    }

    #[test]
    fn pando_crate_edges_are_allowed() {
        let mut v = with_app_pin(good(), "=0.7.1");
        v.push(pkg("pando-rs", true, &[]));
        v.push(pkg(
            "bitacora-pando",
            true,
            &["bitacora-config", "bitacora-core", "pando-rs"],
        ));
        assert_eq!(check(&v), Vec::<String>::new());
    }

    #[test]
    fn testkit_as_normal_dependency_fails() {
        let mut v = with_app_pin(good(), "=0.7.1");
        v.push(pkg("bitacora-testkit", true, &[]));
        for p in &mut v {
            if p.name == "bitacora-markdown" {
                p.deps.insert("bitacora-testkit".into());
            }
        }
        let errs = check(&v);
        assert!(
            errs.iter().any(|e| e.contains("bitacora-testkit")),
            "{errs:?}"
        );
    }
}
