# AG-UI conformance fixtures

Raw SSE bodies (`data: <json>\n\n`, discriminator inside the JSON) copied byte for byte from
Pando's TypeScript SDK (`sdk/typescript/tests/fixtures/agui/`), so the TS, Python and Rust SDKs
replay the same streams. They were hand-authored to the wire format of Pando's
`internal/agui/sse.go` (not captured live; see the upstream README's provenance note). If the
upstream files change, re-copy them and re-run `cargo test -p pando-rs --test conformance`.

- `interrupt-frontend-tool.sse`: run suspended on a frontend tool call (`outcome:"interrupt"`).
- `resume-frontend-tool.sse`: the following run after the tool result (`outcome:"success"`).
- `state-delta-todos-tokenusage-files.sse`: `STATE_DELTA` replace/replace/add sequence.
