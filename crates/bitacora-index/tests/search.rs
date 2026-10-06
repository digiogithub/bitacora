//! Full-text search: ranking, scopes, fuzzy titles, snippets and `search.substring`
//! (BIT-US-0009, BIT-SP-0003.R13-R15).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_index::search::{Scope, SearchHit, SearchOptions, search};
use bitacora_index::{Index, IndexWriter};
use common::{count, env, input, writer};

fn fixture() -> (common::Env, Index, IndexWriter) {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let files = [
        (
            "pages/Project Roadmap.md",
            "alias:: Plan Q3\n\n- the quarterly roadmap review\n- Café menu notes with résumé\n- unrelated gardening tips\n",
        ),
        (
            "pages/Gardening.md",
            "- tomatoes need sunlight\n- the roadmap for the garden\n",
        ),
        (
            "pages/Meeting Notes.md",
            "- discussed budget and roadmap\n- 50% done_ish item\n",
        ),
        (
            "journals/2026_10_01.md",
            "- journal roadmap thoughts\n- 日本語のテキストを検索\n",
        ),
        ("journals/2020_01_01.md", "- old journal roadmap entry\n"),
    ];
    for (p, t) in files {
        w.replace_file(input(p, t)).expect("replace");
    }
    (env, index, w)
}

fn run(index: &Index, q: &str, opts: &SearchOptions) -> Vec<SearchHit> {
    search(&index.reader().expect("reader"), q, opts).expect("search")
}

fn titles(hits: &[SearchHit]) -> Vec<String> {
    hits.iter()
        .map(|h| match h {
            SearchHit::Page { title, .. } => format!("page:{title}"),
            SearchHit::Block { page_title, .. } => format!("block:{page_title}"),
        })
        .collect()
}

#[test]
fn exact_title_and_alias_rank_first() {
    let (_e, index, _w) = fixture();
    let hits = run(&index, "project roadmap", &SearchOptions::default());
    assert_eq!(titles(&hits)[0], "page:Project Roadmap");
    let hits = run(&index, "Plan Q3", &SearchOptions::default());
    assert_eq!(titles(&hits)[0], "page:Project Roadmap");
}

#[test]
fn word_search_ranks_blocks_and_is_accent_and_case_insensitive() {
    let (_e, index, _w) = fixture();
    let hits = run(&index, "CAFE resume", &SearchOptions::default());
    let SearchHit::Block { snippet, .. } = &hits[0] else {
        panic!("expected a block first: {hits:?}");
    };
    assert!(snippet.text.contains("Café"));
    assert_eq!(&snippet.text[snippet.highlights[0].clone()], "Café");
    assert_eq!(&snippet.text[snippet.highlights[1].clone()], "résumé");
}

#[test]
fn operators_and_hostile_input_never_error() {
    let (_e, index, _w) = fixture();
    let o = SearchOptions::default();
    let blocks = |q: &str| {
        run(&index, q, &o)
            .into_iter()
            .filter(|h| matches!(h, SearchHit::Block { .. }))
            .count()
    };
    assert_eq!(blocks("roadmap and garden"), 1);
    assert_eq!(blocks("tomatoes or budget"), 2);
    assert_eq!(blocks("roadmap not garden"), 4);
    for q in [
        "\"",
        "*",
        "NEAR(",
        "a AND",
        ")(",
        "col:roadmap",
        "^roadmap",
        "roadmap -x",
        "'; DROP TABLE blocks;--",
        "\"unterminated phrase",
        "% _ \\",
        "OR",
        "NOT NOT",
        "{roadmap}",
        "roadmap*",
        "\u{0}",
    ] {
        run(&index, q, &o);
    }
    assert!(count(&index, "SELECT count(*) FROM blocks") > 0);
}

#[test]
fn substring_and_cjk_use_trigram_then_like_for_short_input() {
    let (_e, index, _w) = fixture();
    let o = SearchOptions::default();
    // Middle of a word: not found by the word index, found by trigram.
    let hits = run(&index, "admap", &o);
    assert!(!hits.is_empty() && titles(&hits).iter().any(|t| t.starts_with("block:")));
    // CJK (no spaces): trigram.
    let hits = run(&index, "テキスト", &o);
    assert_eq!(titles(&hits), ["block:Oct 1st, 2026"].map(String::from));
    // Two characters: LIKE fallback.
    let hits = run(&index, "本語", &o);
    assert_eq!(hits.len(), 1);
    // LIKE wildcards in the input are literal.
    let hits = run(&index, "50%", &o);
    assert_eq!(hits.len(), 1);
    assert!(run(&index, "d_n", &o).is_empty());
}

#[test]
fn scopes_filter_pages_and_blocks() {
    let (_e, index, _w) = fixture();
    let all = run(&index, "roadmap", &SearchOptions::default());
    let journals = run(
        &index,
        "roadmap",
        &SearchOptions {
            scope: Scope::Journals,
            ..SearchOptions::default()
        },
    );
    assert_eq!(journals.len(), 2);
    assert!(journals.iter().all(|h| matches!(
        h,
        SearchHit::Block {
            is_journal: true,
            ..
        }
    )));
    let pages = run(
        &index,
        "roadmap",
        &SearchOptions {
            scope: Scope::Pages,
            ..SearchOptions::default()
        },
    );
    assert!(pages.iter().all(|h| match h {
        SearchHit::Page { is_journal, .. } | SearchHit::Block { is_journal, .. } => !is_journal,
    }));
    assert!(pages.len() < all.len());
    let page_id: i64 = count(&index, "SELECT id FROM pages WHERE name = 'gardening'");
    let within = run(
        &index,
        "roadmap",
        &SearchOptions {
            scope: Scope::Page(page_id),
            ..SearchOptions::default()
        },
    );
    assert_eq!(within.len(), 1);
    assert!(matches!(&within[0], SearchHit::Block { page_id: p, .. } if *p == page_id));
}

#[test]
fn fuzzy_titles_match_subsequences() {
    let (_e, index, _w) = fixture();
    let hits = run(&index, "mtgnts", &SearchOptions::default());
    assert_eq!(titles(&hits)[0], "page:Meeting Notes");
    let hits = run(&index, "prjrdmp", &SearchOptions::default());
    assert_eq!(titles(&hits)[0], "page:Project Roadmap");
}

#[test]
fn recent_journals_get_a_boost() {
    let (_e, index, _w) = fixture();
    let o = SearchOptions {
        scope: Scope::Journals,
        today: Some(20_261_005),
        ..SearchOptions::default()
    };
    let hits = run(&index, "roadmap", &o);
    let SearchHit::Block { page_title, .. } = &hits[0] else {
        panic!()
    };
    assert_eq!(page_title, "Oct 1st, 2026");
}

#[test]
fn disabling_substring_drops_the_trigram_index_and_falls_back_to_like() {
    let (_e, index, w) = fixture();
    let tri = "SELECT count(*) FROM sqlite_master WHERE name = 'blocks_fts_tri'";
    assert_eq!(count(&index, tri), 1);
    assert!(w.set_substring(false).expect("disable"));
    assert!(!w.set_substring(false).expect("noop"));
    assert_eq!(count(&index, tri), 0);
    // Substring search still works through LIKE.
    let hits = run(&index, "admap", &SearchOptions::default());
    assert!(!hits.is_empty());
    // Writes keep working without the trigram table (triggers were recreated).
    w.replace_file(input("pages/New.md", "- brand new uniqueword here\n"))
        .expect("replace");
    assert_eq!(run(&index, "niqueword", &SearchOptions::default()).len(), 1);
    // Re-enabling rebuilds it from blocks.
    assert!(w.set_substring(true).expect("enable"));
    assert_eq!(count(&index, tri), 1);
    assert_eq!(
        count(&index, "SELECT count(*) FROM blocks_fts_tri_docsize"),
        count(&index, "SELECT count(*) FROM blocks")
    );
    assert_eq!(run(&index, "niqueword", &SearchOptions::default()).len(), 1);
}

#[test]
fn disabled_substring_survives_a_bulk_build() {
    let (_e, index, w) = fixture();
    w.set_substring(false).expect("disable");
    w.begin_bulk(false).expect("bulk");
    w.replace_file(input("pages/Bulk.md", "- bulkword content\n"))
        .expect("replace");
    w.end_bulk().expect("end bulk");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM sqlite_master WHERE name = 'blocks_fts_tri'"
        ),
        0
    );
    assert_eq!(run(&index, "bulkword", &SearchOptions::default()).len(), 1);
}

#[test]
fn empty_and_zero_limit_queries_return_nothing() {
    let (_e, index, _w) = fixture();
    assert!(run(&index, "   ", &SearchOptions::default()).is_empty());
    assert!(
        run(
            &index,
            "roadmap",
            &SearchOptions {
                limit: 0,
                ..SearchOptions::default()
            }
        )
        .is_empty()
    );
}
