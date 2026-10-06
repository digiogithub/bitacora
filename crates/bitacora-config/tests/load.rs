#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Reader, effective config and typed accessors (BIT-US-0056).

use bitacora_config::{
    BulletIndentation, DEFAULT_CONFIG_EDN, DiagnosticKind, Edn, EffectiveConfig, NameFormat,
    PreferredFormat, PreferredWorkflow, read_str,
};

const COMMENTED: &str = include_str!("data/commented-config.edn");

fn read(src: &str) -> Edn {
    read_str(src).expect("valid edn").expect("a form")
}

#[test]
fn reads_scalars_and_collections() {
    let v = read(
        r#"{:a 1 :b -2.5 :c "x\ty" :d [true nil \a] :e #{1 2} :f (fn [x] x) :g #tag 5 :h 10N}"#,
    );
    assert_eq!(v.get("a"), Some(&Edn::Int(1)));
    assert_eq!(v.get("b"), Some(&Edn::Float(-2.5)));
    assert_eq!(v.get("c"), Some(&Edn::str("x\ty")));
    assert_eq!(
        v.get("d"),
        Some(&Edn::Vector(vec![
            Edn::Bool(true),
            Edn::Nil,
            Edn::Char('a')
        ]))
    );
    assert_eq!(v.get("e"), Some(&Edn::Set(vec![Edn::Int(1), Edn::Int(2)])));
    assert_eq!(
        v.get("f"),
        Some(&Edn::List(vec![
            Edn::Symbol("fn".into()),
            Edn::Vector(vec![Edn::Symbol("x".into())]),
            Edn::Symbol("x".into())
        ]))
    );
    assert_eq!(
        v.get("g"),
        Some(&Edn::Tagged("tag".into(), Box::new(Edn::Int(5))))
    );
    assert_eq!(v.get("h"), Some(&Edn::Number("10N".into())));
}

#[test]
fn discards_and_comments_are_skipped() {
    let v = read("{:a 1 #_:b #_2 ; note\n :c #_ #_ x y 3}");
    assert_eq!(v.as_map().map(<[_]>::len), Some(2));
    assert_eq!(v.get("c"), Some(&Edn::Int(3)));
}

#[test]
fn duplicate_keys_are_reported_with_position() {
    let err = read_str("{:a 1\n :b 2\n :a 3}").unwrap_err();
    assert_eq!(err.kind, DiagnosticKind::DuplicateKey);
    assert_eq!((err.line, err.column), (3, 2));
    assert_eq!(
        read_str("#{1 1}").unwrap_err().kind,
        DiagnosticKind::DuplicateKey
    );
}

#[test]
fn invalid_edn_is_reported() {
    for bad in [
        "{:a 1", "{:a 1}}", "{:a}", "\"open", "[1 2)", "#", "{:a #_}",
    ] {
        assert!(read_str(bad).is_err(), "{bad:?} should fail");
    }
    assert_eq!(
        read_str("{:a}").unwrap_err().kind,
        DiagnosticKind::OddMapEntries
    );
}

#[test]
fn empty_document_has_no_form() {
    assert_eq!(read_str("  ; nothing\n").unwrap(), None);
}

#[test]
fn missing_name_format_means_legacy() {
    let cfg = EffectiveConfig::from_texts(None, Some("{:preferred-format \"Markdown\"}"));
    assert_eq!(cfg.name_format(), NameFormat::Legacy);
    assert!(cfg.diagnostics().is_empty());
    let cfg = EffectiveConfig::from_texts(None, None);
    assert_eq!(cfg.name_format(), NameFormat::Legacy);
}

#[test]
fn triple_lowbar_is_detected() {
    let cfg = EffectiveConfig::from_texts(None, Some("{:file/name-format :triple-lowbar}"));
    assert_eq!(cfg.name_format(), NameFormat::TripleLowbar);
}

#[test]
fn graph_wins_and_maps_merge_shallowly() {
    let global = r#"{:macros {"a" "x"} :preferred-workflow :todo}"#;
    let graph = r#"{:macros {"b" "y"} :preferred-workflow :now}"#;
    let cfg = EffectiveConfig::from_texts(Some(global), Some(graph));
    assert_eq!(cfg.preferred_workflow(), PreferredWorkflow::Now);
    let macros = cfg.get("macros").expect("macros");
    assert_eq!(macros.as_map().map(<[_]>::len), Some(2));
    assert_eq!(macros.get("a"), None); // keys are keywords; macro names are strings
    let names: Vec<&str> = macros
        .as_map()
        .expect("map")
        .iter()
        .filter_map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn global_only_values_survive() {
    let cfg = EffectiveConfig::from_texts(Some("{:preferred-workflow :todo}"), Some("{}"));
    assert_eq!(cfg.preferred_workflow(), PreferredWorkflow::Todo);
}

#[test]
fn invalid_graph_config_falls_back_to_defaults_plus_global() {
    let cfg = EffectiveConfig::from_texts(
        Some("{:pages-directory \"gpages\"}"),
        Some("{:pages-directory \"x\" :pages-directory \"y\"}"),
    );
    assert_eq!(cfg.pages_directory(), "gpages");
    assert_eq!(cfg.diagnostics().len(), 1);
    assert_eq!(cfg.diagnostics()[0].kind, DiagnosticKind::DuplicateKey);

    let cfg = EffectiveConfig::from_texts(None, Some("[1 2]"));
    assert_eq!(cfg.diagnostics()[0].kind, DiagnosticKind::NotAMap);
    assert_eq!(cfg.name_format(), NameFormat::Legacy);
}

#[test]
fn accessor_defaults() {
    let cfg = EffectiveConfig::from_texts(None, None);
    assert_eq!(cfg.pages_directory(), "pages");
    assert_eq!(cfg.journals_directory(), "journals");
    assert_eq!(cfg.whiteboards_directory(), "whiteboards");
    assert_eq!(cfg.journal_page_title_format(), "MMM do, yyyy");
    assert_eq!(cfg.journal_file_name_format(), "yyyy_MM_dd");
    assert_eq!(cfg.preferred_format(), PreferredFormat::Markdown);
    assert_eq!(cfg.preferred_format().extension(), "md");
    assert!(cfg.hidden().is_empty());
    assert_eq!(cfg.default_journal_template(), "");
    assert!(cfg.journals_enabled());
    assert!(cfg.whiteboards_enabled());
    assert!(cfg.property_pages_enabled());
    assert!(cfg.property_pages_excludelist().is_empty());
    assert!(cfg.property_separated_by_commas().is_empty());
    assert!(cfg.ignored_page_references_keywords().is_empty());
    assert!(cfg.block_hidden_properties().is_empty());
    assert_eq!(cfg.bullet_indentation(), BulletIndentation::Tab);
    assert!(!cfg.org_insert_file_link());
    assert!(cfg.favorites().is_empty());
    assert_eq!(cfg.default_home(), None);
    assert_eq!(cfg.preferred_workflow(), PreferredWorkflow::Now);
    assert!(cfg.file_sync_ignore_files().is_empty());
    assert_eq!(cfg.meta_version(), 1);
}

#[test]
fn date_formatter_alias_and_precedence() {
    let cfg = EffectiveConfig::from_texts(None, Some("{:date-formatter \"yyyy/MM/dd\"}"));
    assert_eq!(cfg.journal_page_title_format(), "yyyy/MM/dd");
    let cfg = EffectiveConfig::from_texts(
        None,
        Some("{:date-formatter \"a\" :journal/page-title-format \"b\"}"),
    );
    assert_eq!(cfg.journal_page_title_format(), "b");
}

#[test]
fn commented_fixture_parses_and_exposes_every_key() {
    let cfg = EffectiveConfig::from_texts(None, Some(COMMENTED));
    assert!(cfg.diagnostics().is_empty(), "{:?}", cfg.diagnostics());
    assert_eq!(cfg.name_format(), NameFormat::TripleLowbar);
    assert_eq!(cfg.journal_page_title_format(), "yyyy-MM-dd EEEE");
    assert_eq!(cfg.default_journal_template(), "Daily");
    assert_eq!(cfg.preferred_workflow(), PreferredWorkflow::Todo);
    assert_eq!(cfg.hidden(), ["/archived", "drafts/private.md"]);
    assert!(!cfg.property_pages_enabled());
    assert_eq!(cfg.property_pages_excludelist(), ["type", "status"]);
    assert_eq!(cfg.property_separated_by_commas(), ["people", "topics"]);
    assert_eq!(cfg.bullet_indentation(), BulletIndentation::TwoSpaces);
    assert_eq!(cfg.file_sync_ignore_files(), ["^\\.cache/", "\\.tmp$"]);
    assert!(cfg.get("experimental").is_none(), "#_ discards are dropped");
    assert!(matches!(cfg.get("dates"), Some(Edn::Tagged(t, _)) if t == "inst"));
}

#[test]
fn default_config_text_parses() {
    let cfg = EffectiveConfig::from_texts(None, Some(DEFAULT_CONFIG_EDN));
    assert!(cfg.diagnostics().is_empty());
    assert_eq!(cfg.name_format(), NameFormat::TripleLowbar);
    assert_eq!(cfg.preferred_format(), PreferredFormat::Markdown);
}

#[test]
fn hidden_matches_path_prefixes() {
    let cfg = EffectiveConfig::from_texts(None, Some("{:hidden [\"archived\" \"/test.md\"]}"));
    assert!(cfg.is_hidden("archived/old.md"));
    assert!(cfg.is_hidden("/archived/old.md"));
    assert!(cfg.is_hidden("test.md"));
    assert!(!cfg.is_hidden("pages/archived.md"));
    assert!(!cfg.is_hidden("other.md"));
}

#[test]
fn preferred_format_org_and_keyword() {
    for src in ["{:preferred-format \"Org\"}", "{:preferred-format :org}"] {
        let cfg = EffectiveConfig::from_texts(None, Some(src));
        assert_eq!(cfg.preferred_format(), PreferredFormat::Org);
    }
}

#[test]
fn load_from_disk_reports_path_in_diagnostics() {
    let dir = std::env::temp_dir().join(format!("bitacora-config-test-{}", std::process::id()));
    let logseq = dir.join("logseq");
    std::fs::create_dir_all(&logseq).expect("mkdir");
    std::fs::write(logseq.join("config.edn"), "{:a 1 :a 2}").expect("write");
    let global = dir.join("global.edn");
    std::fs::write(&global, "{:journals-directory \"j\"}").expect("write");
    let cfg = EffectiveConfig::load(&dir, Some(&global));
    assert_eq!(cfg.journals_directory(), "j");
    assert_eq!(cfg.diagnostics().len(), 1);
    assert!(cfg.diagnostics()[0].path.as_deref() == Some(&*logseq.join("config.edn")));
    // Missing files are not errors.
    let missing = EffectiveConfig::load(&dir.join("nope"), None);
    assert!(missing.diagnostics().is_empty());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// Dev-only black-box check against a local Logseq checkout; the template is never copied.
#[test]
#[ignore = "needs a Logseq checkout (LOGSEQ_CHECKOUT or ../logseq)"]
fn logseq_template_parses() {
    let root = std::env::var("LOGSEQ_CHECKOUT")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../logseq").to_owned());
    let path = format!("{root}/src/resources/templates/config.edn");
    let text = std::fs::read_to_string(&path).expect("template");
    let cfg = EffectiveConfig::from_texts(None, Some(&text));
    assert!(cfg.diagnostics().is_empty(), "{:?}", cfg.diagnostics());
}
