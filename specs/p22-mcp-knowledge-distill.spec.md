spec: task
name: "P22: mempal_knowledge_distill MCP tool"
inherits: project
tags: [memory, knowledge, distill, mcp]
---

## Intent

P18 added CLI-only deterministic knowledge distillation from evidence refs, and P21 exposed promotion gate checks to MCP agents. P22 closes the agent-facing Stage-1 loop by adding a read/write MCP tool that creates candidate knowledge drawers from existing evidence refs without using an LLM, auto-promoting, or changing the Phase-1 drawer model.

## Decisions

- Add one MCP tool named `mempal_knowledge_distill`.
- The MCP tool reuses the same shared distill implementation as CLI `mempal knowledge distill`; CLI and MCP must not drift.
- Request fields are `statement`, `content`, `tier`, `supporting_refs`, optional `counterexample_refs`, optional `teaching_refs`, optional `domain`, optional `field`, optional `wing`, optional `room`, optional `scope_constraints`, optional `trigger_hints`, optional `cwd`, optional `importance`, and optional `dry_run`.
- Default request values match the CLI: `domain="project"`, `field="general"`, `wing="mempal"`, `room="knowledge"`, `importance=3`, and `dry_run=false`.
- Distill always creates `memory_kind=knowledge`, `status=candidate`, and empty `verification_refs`.
- Distill only allows candidate `tier=dao_ren|qi`.
- All role refs must be existing evidence drawer ids; refs to knowledge drawers, missing drawers, or non-`drawer_` ids are invalid-params errors.
- Dry-run returns the deterministic `drawer_id` and `created=false` without building an embedder, inserting drawers/vectors, or writing audit.
- If the computed drawer already exists, the tool returns `created=false` without inserting duplicate drawer/vector rows and without appending audit.
- Successful non-dry-run create embeds `content`, inserts the drawer/vector, appends a `knowledge_distill` audit entry, and returns `created=true`.
- The tool is local-only and deterministic; it must not call LLMs, external research tools, or network services beyond the configured embedder used by existing ingest/distill paths.
- `MEMORY_PROTOCOL`, docs, and repo instructions list `mempal_knowledge_distill` as the MCP surface for creating candidate knowledge from evidence.

## Boundaries

### Allowed Changes
- `src/knowledge_distill.rs`
- `src/lib.rs`
- `src/main.rs`
- `src/mcp/tools.rs`
- `src/mcp/server.rs`
- `src/core/protocol.rs`
- `tests/knowledge_lifecycle.rs`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/plans/**`

### Forbidden
- Do not add tables, columns, triggers, or migrations.
- Do not add MCP promote or demote tools.
- Do not change `mempal_knowledge_gate` behavior.
- Do not change `mempal_context` response schema or ordering.
- Do not change `mempal_search` request or response schema.
- Do not auto-promote distilled knowledge.
- Do not call an LLM or external research tool.
- Do not introduce new dependencies.

## Acceptance Criteria

Scenario: MCP distill creates candidate knowledge from evidence
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_creates_candidate_knowledge
  Given one existing evidence drawer id
  When an MCP client calls `mempal_knowledge_distill` with `statement`, `content`, `tier="dao_ren"`, and `supporting_refs=[evidence_id]`
  Then the response has `created=true`
  And the response contains a deterministic `drawer_id`
  And the stored drawer has `memory_kind=knowledge`, `tier=dao_ren`, `status=candidate`, and `supporting_refs=[evidence_id]`
  And default `mempal_context` does not include the candidate drawer

Scenario: MCP distill dry-run is deterministic and read-only
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_dry_run_no_write
  Given one existing evidence drawer id
  When the same dry-run `mempal_knowledge_distill` request is called twice
  Then both responses have the same `drawer_id`
  And both responses have `dry_run=true` and `created=false`
  And drawer count, vector count, schema version, and audit line count do not change

Scenario: MCP distill rejects unsupported candidate tier
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_rejects_dao_tian_candidate
  Given one existing evidence drawer id
  When an MCP client calls `mempal_knowledge_distill` with `tier="dao_tian"`
  Then the call fails with invalid params
  And the error says distill only allows candidate `dao_ren` or `qi`

Scenario: MCP distill validates supporting refs
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_rejects_missing_supporting_refs
  Given no supporting refs
  When an MCP client calls `mempal_knowledge_distill`
  Then the call fails with invalid params
  And the error mentions `supporting_refs`
  Given one supporting ref that points to a knowledge drawer
  When an MCP client calls `mempal_knowledge_distill`
  Then the call fails with invalid params
  And the error says refs must point to evidence drawers

Scenario: MCP distill stores trigger hints as bias metadata
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_stores_trigger_hints
  Given one existing evidence drawer id
  When an MCP client calls `mempal_knowledge_distill` with `trigger_hints.intent_tags=["debugging"]`, `trigger_hints.workflow_bias=["reproduce-first"]`, and `trigger_hints.tool_needs=["cargo-test"]`
  Then the stored drawer preserves those trigger hints
  And `MEMORY_PROTOCOL` still says trigger hints are bias metadata only

Scenario: MCP distill is idempotent for existing drawer
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_distill_existing_drawer_no_duplicate_or_audit
  Given one existing evidence drawer id
  When an MCP client calls the same non-dry-run `mempal_knowledge_distill` request twice
  Then the first response has `created=true`
  And the second response has `created=false`
  And drawer count and vector count do not increase after the second call
  And no second audit entry is appended

Scenario: MCP tool registry and protocol include mempal_knowledge_distill
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_mempal_knowledge_distill
  Given the MCP server tool registry and `MEMORY_PROTOCOL`
  When they are inspected
  Then the tool registry contains `mempal_knowledge_distill`
  And the tool description mentions candidate knowledge from evidence
  And `MEMORY_PROTOCOL` tool list mentions `mempal_knowledge_distill`

## Out of Scope

- MCP promote/demote tools.
- LLM-generated summaries.
- Research tool integration.
- Enforcing promotion gates inside distill.
- Phase-2 `knowledge_cards` tables.
