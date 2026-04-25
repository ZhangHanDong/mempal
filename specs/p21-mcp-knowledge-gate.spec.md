spec: task
name: "P21: mempal_knowledge_gate MCP tool"
inherits: project
tags: [memory, knowledge, lifecycle, gate, mcp]
---

## Intent

P20 added a read-only `mempal knowledge gate` CLI command, but MCP-connected agents still cannot evaluate promotion readiness without shelling out. P21 exposes the same deterministic promotion gate policy through a read-only MCP tool so agents can inspect whether a knowledge drawer is eligible for promotion before proposing or performing lifecycle work.

## Decisions

- Add one MCP tool named `mempal_knowledge_gate`.
- The MCP tool reuses the same evaluator as CLI `mempal knowledge gate`; CLI and MCP must not drift.
- Request fields are `drawer_id`, optional `target_status`, optional `reviewer`, and optional `allow_counterexamples`.
- Response fields match the P20 JSON gate report: `drawer_id`, `tier`, `status`, `target_status`, `allowed`, `reasons`, `requirements`, and `evidence_counts`.
- Default `target_status` remains tier-derived: `dao_tian -> canonical`; `dao_ren`, `shu`, and `qi -> promoted`.
- The MCP tool is pure read: it must not mutate drawers, vectors, triples, tunnels, inbox files, schema version, or audit log.
- Invalid target drawers, invalid target status, malformed refs, missing refs, and refs pointing to knowledge drawers are MCP invalid-params errors.
- `MEMORY_PROTOCOL` tool list mentions `mempal_knowledge_gate` and describes it as a read-only promotion readiness check.
- Docs and repo instructions list the new MCP tool and P21 completion state.

## Boundaries

### Allowed Changes
- `src/knowledge_gate.rs`
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
- Do not add a schema migration.
- Do not add MCP promote or demote tools.
- Do not change existing CLI gate output shape.
- Do not change `mempal_context` response schema or ordering.
- Do not change `mempal_search` request or response schema.
- Do not auto-promote knowledge from the gate result.

## Acceptance Criteria

Scenario: MCP gate allows dao_ren promotion
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_allows_dao_ren_promotion
  Given one candidate `dao_ren` knowledge drawer with two supporting evidence refs and one verification evidence ref
  When an MCP client calls `mempal_knowledge_gate` with only `drawer_id`
  Then the response has `target_status="promoted"`
  And `allowed=true`
  And `evidence_counts.supporting=2`
  And `evidence_counts.verification=1`

Scenario: MCP gate rejects missing verification
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_rejects_missing_verification
  Given one candidate `dao_ren` knowledge drawer with two supporting evidence refs and no verification refs
  When an MCP client calls `mempal_knowledge_gate`
  Then the response has `allowed=false`
  And `reasons` mentions missing verification evidence
  And no drawer, vector, schema, or audit-log state changes

Scenario: MCP gate requires reviewer for dao_tian canonical
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_requires_reviewer_for_dao_tian
  Given one canonical-eligible `dao_tian` knowledge drawer with three supporting refs, two verification refs, and one teaching ref
  When an MCP client calls `mempal_knowledge_gate` without `reviewer`
  Then the response has `target_status="canonical"`
  And `allowed=false`
  And `reasons` mentions reviewer is required
  When the same request includes `reviewer="alex"`
  Then the response has `allowed=true`

Scenario: MCP gate blocks counterexamples by default
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_blocks_counterexamples_by_default
  Given one `shu` knowledge drawer with one supporting ref, one verification ref, and one counterexample ref
  When an MCP client calls `mempal_knowledge_gate` without `allow_counterexamples`
  Then the response has `allowed=false`
  And `reasons` mentions counterexample refs
  When the same request sets `allow_counterexamples=true`
  Then the response has `allowed=true`

Scenario: MCP gate rejects evidence drawer targets
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_rejects_evidence_drawer
  Given an evidence drawer id
  When an MCP client calls `mempal_knowledge_gate` for that drawer
  Then the call fails with invalid params
  And the error mentions `knowledge drawer`

Scenario: MCP gate validates refs are evidence drawers
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_gate_validates_role_refs
  Given one knowledge drawer whose `verification_refs` contains another knowledge drawer id
  When an MCP client calls `mempal_knowledge_gate`
  Then the call fails with invalid params
  And the error mentions refs must point to evidence drawers

Scenario: MCP tool registry and protocol include mempal_knowledge_gate
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_mempal_knowledge_gate
  Given the MCP server tool registry and `MEMORY_PROTOCOL`
  When they are inspected
  Then the tool registry contains `mempal_knowledge_gate`
  And the tool description mentions read-only promotion readiness
  And `MEMORY_PROTOCOL` tool list mentions `mempal_knowledge_gate`

## Out of Scope

- Enforcing the gate inside `mempal knowledge promote`.
- Adding MCP lifecycle mutation tools.
- LLM-based scoring or confidence derivation.
- Cross-drawer automatic distillation.
- Phase-2 `knowledge_cards` tables.
