spec: task
name: "P23: MCP knowledge lifecycle mutation tools"
inherits: project
tags: [memory, knowledge, lifecycle, gate, mcp]
---

## Intent

P21 exposes promotion gate checks to MCP agents and P22 lets agents distill candidate knowledge from evidence, but the final lifecycle mutation still requires shelling out to the CLI. P23 exposes the P17 promote/demote lifecycle through MCP while preserving the P20 gate discipline: MCP promotion must pass the deterministic gate before mutating a drawer.

## Decisions

- Add MCP tools `mempal_knowledge_promote` and `mempal_knowledge_demote`.
- CLI and MCP lifecycle mutations must reuse one shared implementation; they must not drift.
- `mempal_knowledge_promote` request fields:
  - `drawer_id`
  - `status`, limited to `promoted|canonical`
  - `verification_refs`, one or more evidence drawer ids
  - `reason`
  - optional `reviewer`
  - optional `allow_counterexamples`, default `false`
- MCP promote appends verification refs with stable de-duplication before evaluating the promotion gate.
- MCP promote must run the same deterministic gate policy as P20 against the effective post-ref drawer before mutation.
- If the promotion gate rejects, MCP promote returns invalid params and must not update status, refs, vectors, schema, or audit log.
- `mempal_knowledge_demote` request fields:
  - `drawer_id`
  - `status`, limited to `demoted|retired`
  - `evidence_refs`, one or more evidence drawer ids
  - `reason`
  - `reason_type`, limited to `contradicted|obsolete|superseded|out_of_scope|unsafe`
- MCP demote appends evidence refs to `counterexample_refs` with stable de-duplication.
- Lifecycle tools only accept `memory_kind=knowledge`.
- All lifecycle refs must be existing evidence drawer ids.
- Lifecycle tools do not change content, statement, tier, anchor metadata, vectors, FTS, triples, tunnels, or schema.
- Successful lifecycle mutations append audit entries matching the CLI command names: `knowledge_promote` and `knowledge_demote`.
- `MEMORY_PROTOCOL`, docs, and repo instructions list both lifecycle MCP tools and state that promotion is gate-enforced.

## Boundaries

### Allowed Changes

- `src/knowledge_lifecycle.rs`
- `src/knowledge_gate.rs`
- `src/lib.rs`
- `src/main.rs`
- `src/mcp/tools.rs`
- `src/mcp/server.rs`
- `src/core/protocol.rs`
- `tests/**`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/plans/**`

### Forbidden

- Do not add tables, columns, triggers, or migrations.
- Do not add Phase-2 `knowledge_cards`.
- Do not add automatic promotion from distill.
- Do not change context assembly ordering or active-status rules.
- Do not change `mempal_knowledge_gate` response shape.
- Do not re-embed or rewrite vectors.
- Do not call LLMs, external research tools, or network services.
- Do not introduce new dependencies.

## Acceptance Criteria

Scenario: MCP promote updates status after gate pass
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_promote_updates_status_after_gate_pass
  Given one candidate `dao_ren` knowledge drawer with two supporting evidence refs
  And one verification evidence drawer exists
  When an MCP client calls `mempal_knowledge_promote` with `status="promoted"` and that verification ref
  Then the response reports old status `candidate` and new status `promoted`
  And the stored drawer status is `promoted`
  And `verification_refs` contains the supplied ref
  And one `knowledge_promote` audit entry is appended

Scenario: MCP promote rejects gate failure without mutation
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_promote_rejects_gate_failure_without_mutation
  Given one candidate `dao_ren` knowledge drawer with only one supporting evidence ref
  And one verification evidence drawer exists
  When an MCP client calls `mempal_knowledge_promote`
  Then the call fails with invalid params
  And the error mentions promotion gate failed
  And drawer status, refs, schema, vector rows, and audit log remain unchanged

Scenario: MCP demote updates status and counterexample refs
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_demote_updates_status_and_counterexample_refs
  Given one promoted `shu` knowledge drawer
  And one counterexample evidence drawer exists
  When an MCP client calls `mempal_knowledge_demote` with `status="demoted"` and `reason_type="contradicted"`
  Then the response reports old status `promoted` and new status `demoted`
  And the stored drawer status is `demoted`
  And `counterexample_refs` contains the supplied evidence ref
  And one `knowledge_demote` audit entry is appended

Scenario: MCP lifecycle rejects evidence drawer targets
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_lifecycle_rejects_evidence_drawer_targets
  Given one evidence drawer id
  When an MCP client calls promote or demote for that drawer
  Then the call fails with invalid params
  And the error mentions lifecycle requires a knowledge drawer

Scenario: MCP lifecycle validates refs are evidence drawers
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_lifecycle_validates_refs_are_evidence_drawers
  Given one knowledge drawer id
  And a lifecycle ref points to another knowledge drawer
  When an MCP client calls promote or demote with that ref
  Then the call fails with invalid params
  And the error mentions lifecycle refs must point to evidence drawers

Scenario: MCP tool registry and protocol include lifecycle tools
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_knowledge_lifecycle_tools
  Given the MCP tool registry and `MEMORY_PROTOCOL`
  When they are inspected
  Then the registry contains `mempal_knowledge_promote` and `mempal_knowledge_demote`
  And `MEMORY_PROTOCOL` says MCP promotion is gate-enforced

## Out of Scope

- Automatic distill-to-promote.
- LLM or evaluator scoring.
- REST lifecycle endpoints.
- Anchor publication APIs.
- Phase-2 lifecycle event tables.
