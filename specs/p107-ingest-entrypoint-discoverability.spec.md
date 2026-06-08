spec: task
name: "P107: ingest entrypoint discoverability"
inherits: project
tags: [mind-model, ingest, discoverability, protocol, docs]
estimate: 0.5d
---

## Intent

P107 makes the evidence-vs-knowledge ingest boundary discoverable to a
first-time agent BEFORE it triggers the `evidence drawer does not allow
knowledge-only fields` rejection. Today that boundary is enforced only at
validation time (`src/mcp/server.rs`), but the surfaces an agent reads before
calling — the `mempal_ingest` tool description, the `IngestRequest` JSON schema
field docs, the rejection message itself, and the embedded MEMORY_PROTOCOL —
are silent about it. A drawer storing the lesson is pull-based and a first-time
agent will not search for it; this contract must live on push-based surfaces.
P107 is documentation/discoverability only: it changes no schema, adds no
table, and changes no ingest behavior, defaults, or validation rules.

## Decisions

- The `mempal_ingest` tool `description` (`src/mcp/server.rs`) states that the
  default creates a raw EVIDENCE drawer (content plus wing/room/importance/
  source only), that knowledge-only fields are rejected on an evidence drawer,
  and that distilling evidence into a governed rule uses
  `mempal_knowledge_distill`.
- The knowledge-only fields on `IngestRequest` (`src/mcp/tools.rs`) —
  `memory_kind`, `statement`, `tier`, `status`, `supporting_refs`,
  `counterexample_refs`, `teaching_refs`, `verification_refs`,
  `scope_constraints`, `trigger_hints` — gain Rust doc comments so the derived
  MCP JSON schema exposes that they are knowledge-only and steers to
  `mempal_knowledge_distill`. Fields valid on evidence (domain, field,
  provenance, anchor_*, cwd, source, importance, dry_run, diary_rollup) are not
  relabeled.
- The evidence rejection error (`src/mcp/server.rs`) is made remedial: it names
  the knowledge-only fields and tells the caller to omit them for an evidence
  drawer or use `mempal_knowledge_distill`.
- The embedded MEMORY_PROTOCOL (`src/core/protocol.rs`) documents the
  evidence-vs-knowledge entrypoint split: default `mempal_ingest` writes
  evidence; knowledge-only fields are rejected there; distilling evidence into
  typed knowledge goes through `mempal_knowledge_distill` then gate/promote.
- No change to validation logic, accepted/rejected field sets, defaults, schema
  version, or database behavior. The only observable runtime changes are the
  text of the tool description, the schema field docs, the error message, and
  the protocol string.

## Boundaries

### Allowed Changes
- src/mcp/server.rs
- src/mcp/tools.rs
- src/core/protocol.rs
- specs/p107-ingest-entrypoint-discoverability.spec.md
- docs/plans/2026-06-09-p107-ingest-entrypoint-discoverability.md
- CHANGELOG.md
- Cargo.toml
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not change the set of fields accepted or rejected for evidence vs
  knowledge ingest.
- Do not change ingest defaults, validation logic, schema version, or any
  database behavior.
- Do not add new tools, request fields, tables, or feature flags.
- Do not modify the CLI `mempal ingest` entrypoint behavior (CLI text is out of
  scope for this patch).

## Acceptance Criteria

Scenario: evidence ingest with a knowledge-only field returns a remedial error
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_ingest_evidence_rejects_knowledge_fields_with_remedy
  Given a default (evidence) ingest carrying a `statement`
  When it is validated by the MCP server
  Then the call is rejected
  And the error message names `mempal_knowledge_distill`

Scenario: ingest tool description steers to the distill entrypoint
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_ingest_tool_description_steers_to_distill
  Given the MCP tool registry
  When the `mempal_ingest` tool description is read
  Then it mentions an evidence drawer default
  And it mentions `mempal_knowledge_distill`

Scenario: ingest schema documents knowledge-only fields
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_ingest_schema_documents_knowledge_only_fields
  Given the `mempal_ingest` tool input schema
  When it is serialized
  Then it documents that the knowledge-only fields are not for an evidence drawer
  And it mentions `mempal_knowledge_distill`

Scenario: protocol documents the evidence vs knowledge entrypoint
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_protocol_documents_evidence_vs_knowledge_entrypoint
  Given the embedded MEMORY_PROTOCOL
  When it is read
  Then it states that default ingest writes evidence
  And it directs distilling evidence to `mempal_knowledge_distill`

## Out of Scope

- Any change to which fields are legal on evidence vs knowledge drawers.
- The CLI `mempal ingest` help text and CLI-side error wording.
- New schema, tables, embeddings, or tunable thresholds.
- Auto-distill, auto-promote, or any change to governance (P77/P80 unchanged).
