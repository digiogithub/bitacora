---
created_at: 2026-10-06T22:43:53.355954021Z
updated_at: 2026-10-06T22:43:53.355954021Z
tags:
    - change
    - app
    - themes
    - i18n
---
# BIT-US-0108 Themes, custom.css subset and translations

Continues [[bitacora-full-development-plan]], builds on [[bit-us-0014-0024-0025-app-shell]] and [[bit-us-0107-0016-0015-settings-ui-keychain]]; see [[gpui-and-gpui-kit]].

## What changed (bitacora-app)
- **Themes (BIT-T-0333)**: `assets/themes/bitacora.json` (Paper, Solarized Light, Midnight, Solarized Dark) registered in the GPUI Kit `ThemeRegistry` at `theme::install`; user theme JSON from `<config_dir>/themes/*.json` (`theme::reload_user_themes`, errors listed in Settings; new theme names only, edits to an already-registered name need a restart because the registry has no remove API). Light/dark/system and per-mode theme selection already existed (`theme::apply`, `set_theme_name`, `build_menu`). `AppDirs::themes_dir`.
- **custom.css (BIT-T-0334)**: `src/custom_css.rs` tolerant parser (comments, strings, nested/unbalanced braces, at-rules skipped, `var()` resolution, hex/rgb/hsl/named colours, px/pt/rem/em sizes). Supported: `--ls-primary/secondary/tertiary-background-color`, `--ls-primary/secondary-text-color`, `--ls-link-text-color`, `--ls-block-bullet-color`, `--ls-border-color`, `--ls-selection-background-color`, `--ls-font-family`, plus `font-family`, `font-size`, `color`, `background-color` on `:root/html/body/.editor`; mode scopes `.dark-theme`/`.white-theme`/`[data-theme=..]`. Everything else yields a `Diagnostic` (line, subject, reason) shown in Settings > Appearance. Applied through `Theme::update` (reconciles component tokens) in `theme::apply`; the font size from Settings wins over the stylesheet. Block bullets read `theme::bullet_color()` (block_view.rs). `theme::set_graph_css` is called from `Workspace::open_graph/close_session`; the file is polled every 2 s (`check_css`, mtime+size) because the watcher deliberately ignores `logseq/custom.css` (no runtime event exists). The file is never written.
- **Translations (BIT-T-0335)**: `src/i18n.rs` (languages, system-locale resolution via `sys-locale`, `refusal()` mapping core `Refusal` to localized text), `AppSettings.language` (None = follow system), language dropdown in Settings > Appearance, `theme::set_language`. New locale files `es.yml`, `settings.es.yml`, `widgets.es.yml`, `editor.{en,es}.yml` (editor notices/refusals, `app.quit`, `sync.onboarding.*`, `sync.recovery.*`). Hard-coded strings moved to locales: editor/view.rs notices, editor/element.rs conflict bar, editor/completion.rs, sync_prefs::actionable, session::recovery_message, app menu Quit. Undo/audit transaction labels ("Delete page", "Split block"...) stay English on purpose: they are persisted data.
- Tests: custom_css unit tests (5), theme gpui tests (bundled + user themes, custom.css mapping/mode/hot reload/close), `lib.rs::every_key_exists_in_all_locales_with_the_same_placeholders`, `tests/language.rs` (separate binary because the locale is process-global). In `cfg(test)` an unset language resolves to English so unit tests do not depend on the host OS locale.
- Dependency: `sys-locale 0.3` (already in the lock; cargo deny/machete clean). typos.toml excludes Spanish locale files.

## Verification
fmt, typos, clippy workspace -D warnings clean, `cargo deny check`, `cargo xtask check-deps`, `cargo machete`; `cargo test --workspace --locked --no-fail-fast`: 1524 passed, 0 failed. Xvfb+lavapipe screenshots viewed: light, dark, custom.css applied (tinted sidebar/status, coloured bullets, serif 18px), Spanish UI, Settings > Appearance in Spanish with the diagnostics list.

## Follow-ups
Slash-command labels (editor/commands.rs table) and a few notices in editor/view/slash.rs are still English (owned by the concurrent slash/DnD story). Editor-internal bullet rendering does not yet read `theme::bullet_color()`.
