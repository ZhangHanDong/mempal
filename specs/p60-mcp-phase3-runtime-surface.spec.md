spec: task
name: "P60: MCP Phase-3 runtime surface"
inherits: project
tags: ["phase-3", "mcp", "runtime-adoption"]
---

## Intent

P54-P59 exposed Phase-3 runtime adoption evidence through CLI, but agent runtime
primarily uses MCP. P60 adds one minimal MCP surface, `mempal_phase3`, so agents
can record runtime adoption events, inspect adoption evidence, evaluate Phase-3
readiness gates, and validate external research report contracts without
shelling out.

## Decisions

- Add one MCP tool named `mempal_phase3`.
- Use an `action` field instead of multiple tools.
- Supported actions are `record`, `list`, `stats`, `gate`, and `research_validate_plan`.
- `record` appends a `runtime_adoption_events` row using the existing schema v9 table.
- `list`, `stats`, `gate`, and `research_validate_plan` are read-only.
- MCP `research_validate_plan` accepts a JSON `report` object rather than a filesystem path.
- The tool must preserve P56-P59 boundaries: no default card context enablement, no card vector schema, no evaluator lifecycle mutation, and no promoted/canonical research ingestion.

## Boundaries

### Allowed Changes

- `src/mcp/server.rs`
- `src/mcp/tools.rs`
- `src/core/protocol.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-05-p60-mcp-phase3-runtime-surface.md`
- `specs/p60-mcp-phase3-runtime-surface.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden

- Do not add schema v10 or modify `runtime_adoption_events`.
- Do not add card vector tables or card embeddings.
- Do not change `mempal context` / `mempal_context` default `include_cards` behavior.
- Do not let evaluator signals mutate knowledge drawer or knowledge card lifecycle state.
- Do not ingest research reports or create promoted/canonical knowledge from this MCP tool.
- Do not add REST API endpoints.

## Acceptance Criteria

Scenario: MCP records and summarizes Phase-3 adoption events
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_record_stats_and_gate_actions
  Level: unit
  Given an empty test database at schema v9
  When `mempal_phase3` records three `card_context` accepted events for `include_cards`
  Then the response returns the created event ids
  And `action=stats` reports `accepted=3` and `rollbacks=0`
  And `action=gate` for `card-context-default` reports `ready=true`

Scenario: MCP research validation is read-only
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_research_validate_plan_is_read_only
  Level: unit
  Given a valid external research report JSON object
  When `mempal_phase3` runs `action=research_validate_plan`
  Then the response reports `valid=true`
  And the drawer count is unchanged
  And no runtime adoption event is inserted

Scenario: invalid MCP action is rejected without mutation
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_rejects_invalid_action_without_mutation
  Level: unit
  Given a database with zero runtime adoption events
  When `mempal_phase3` is called with unsupported `action=promote`
  Then the request fails with an invalid action error
  And the runtime adoption event count remains unchanged

Scenario: MCP tool registry and protocol advertise Phase-3 surface
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface
  Level: unit
  Given the MCP server tool registry
  When listing available tools
  Then `mempal_phase3` exists
  And its description lists `record/list/stats/gate/research_validate_plan`
  And `MEMORY_PROTOCOL` mentions `mempal_phase3` and runtime adoption evidence

## Out of Scope

- Automatic runtime adoption recording protocol.
- Card context default-on behavior.
- Card-level embeddings.
- Evaluator advisory API response contracts beyond the read-only gate.
- Research adapter apply/ingest behavior.
