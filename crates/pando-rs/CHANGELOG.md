# Changelog

All notable changes to `pando-rs`. Format follows Keep a Changelog; versions follow semver
(see README, "Versioning").

## [0.1.0] - unreleased

First version. Verified against Pando v1.2.11.

### Added
- `PandoClient`, `PandoConfig`, redacted `Token`, `/health` server info.
- `kb::KbClient`: upsert, delete, search, reindex (409 -> `Error::ReindexRunning`), embedding
  model list and embedder test (`X-Pando-Token` auth).
- `agui::AguiClient`: info, healthz, run / run_text (tolerant SSE `Event` stream), attach,
  cancel_run, list_threads, thread_messages, delete_thread (`Authorization: Bearer` auth,
  stream idle timeout).
- `agui::Thread` (transcript, STATE_SNAPSHOT/STATE_DELTA reduction, interrupts) and
  `agui::hitl` helpers for permission prompts and `AskUserQuestion`.
- Conformance: shared AG-UI SSE replay fixtures and `#[ignore]` live tests against a real
  `pando agui-serve`.
