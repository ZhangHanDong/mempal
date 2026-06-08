# P107 Implementation Plan — ingest entrypoint discoverability

Spec: `specs/p107-ingest-entrypoint-discoverability.spec.md`
Target release: 0.6.1 (quality / discoverability patch, no behavior change)

## Context

Codex hit `evidence drawer does not allow knowledge-only fields` (MCP -32602)
when it passed a distilled `statement` + `supporting_refs` through
`mempal_ingest`, which defaults to `memory_kind="evidence"`. The boundary is
correct, but invisible until failure. A drawer storing the lesson is pull-based
and a first-time agent will not search for it before its first call. The fix
puts the evidence-vs-knowledge contract on push-based surfaces the agent already
reads: tool description, schema field docs, the error message, and the embedded
MEMORY_PROTOCOL. Documentation/discoverability only — zero behavior change.

## Tasks

- [ ] T1. Spec `specs/p107-ingest-entrypoint-discoverability.spec.md` (done first).
- [ ] T2. Plan (this file).
- [ ] T3. `src/mcp/server.rs` — `mempal_ingest` `#[tool(description=...)]`: append
      evidence-default + knowledge-only-rejection + steer to
      `mempal_knowledge_distill`.
- [ ] T4. `src/mcp/tools.rs` — add doc comments to the 9 knowledge-only fields +
      `memory_kind` on `IngestRequest` so the derived JSON schema exposes the
      contract. Leave evidence-legal fields untouched.
- [ ] T5. `src/mcp/server.rs` — make the evidence rejection error remedial:
      name the fields, tell caller to omit or use `mempal_knowledge_distill`.
- [ ] T6. `src/core/protocol.rs` — extend Rule 4 (SAVE AFTER DECISIONS) with the
      evidence-vs-knowledge entrypoint split pointing to `mempal_knowledge_distill`.
- [ ] T7. Tests in `src/mcp/server.rs` tests module:
      - `test_mcp_ingest_evidence_rejects_knowledge_fields_with_remedy`
      - `test_mcp_ingest_tool_description_steers_to_distill`
      - `test_mcp_ingest_schema_documents_knowledge_only_fields`
      - `test_mcp_protocol_documents_evidence_vs_knowledge_entrypoint`
- [ ] T8. `cargo test` green; `cargo clippy` clean; `agent-spec lint` >= 0.7.
- [ ] T9. Version bump to 0.6.1: `Cargo.toml` + `CHANGELOG.md`.
- [ ] T10. Inventory sync: AGENTS.md + CLAUDE.md spec/plan tables.
- [ ] T11. Commit on a branch, `cargo publish`, tag v0.6.1.

## Verification

- Acceptance: the four `cargo test --lib mcp::server::tests::*` filters in the spec.
- Regression: full `cargo test` (no behavior change expected anywhere else).
- Manual: a default `mempal_ingest` with only `content` still succeeds; an
  evidence ingest with `statement` still fails — now with a remedial message.

## Rollback

Pure text changes. Revert the commit to restore prior wording; no data
migration, no schema change, nothing to undo in any database.
