spec: task
name: "P68: MCP research ingest plan"
inherits: project
tags: [phase-3, research, adapter, evidence, mcp]
---

## Intent

P67 added an explicit CLI dry-run/apply path for converting external research
reports into evidence drawers. P68 exposes the same dry-run planning semantics
through `mempal_phase3` so agent runtimes can inspect planned evidence refs and
distill suggestions without granting MCP write access or bypassing P49/P50
governance.

## Decisions

- Add MCP action `mempal_phase3 action=research_ingest_plan`.
- The request accepts the same inline `report` JSON object used by
  `research_validate_plan`.
- The response returns a `research_ingest_plan` object with the P67 dry-run
  shape: `valid`, `writes=false`, report metadata, planned evidence drawers,
  candidate insight distill suggestions, and validation errors.
- MCP `research_ingest_plan` is always read-only and never exposes `execute`.
- CLI `mempal phase3 research-ingest-plan` and MCP `research_ingest_plan` share
  the same pure planning logic.
- `research_validate_plan` remains unchanged and continues to return only
  validation counts.

## Boundaries

### Allowed Changes
- specs/p68-mcp-research-ingest-plan.spec.md
- docs/plans/2026-05-13-p68-mcp-research-ingest-plan.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/core/phase3.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add schema v10 or any table.
- Do not add MCP `execute`, write mode, or evidence drawer creation.
- Do not create vectors from the MCP action.
- Do not create `memory_kind=knowledge` drawers from research reports.
- Do not create `dao_tian`, `canonical`, or `promoted` knowledge.
- Do not bypass `mempal knowledge distill`, lifecycle gates, or human review.
- Do not change P67 CLI observable behavior.
- Do not change `research_validate_plan` response semantics.

## Acceptance Criteria

Scenario: MCP research ingest plan returns dry-run evidence refs without writing
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_research_ingest_plan_is_read_only --lib
    Targets: MCP JSON response shape, dry-run semantics, drawer/event no-write checks.
  Given a valid inline research report with one finding and one candidate insight
  When calling `mempal_phase3` with `action=research_ingest_plan`
  Then the call succeeds
  And the response has `research_ingest_plan.valid=true`
  And `research_ingest_plan.writes=false`
  And it returns one planned evidence drawer
  And it returns one candidate insight suggestion
  And the suggestion includes a `mempal knowledge distill` command
  And no drawer or runtime adoption event is created

Scenario: MCP research ingest plan reports invalid input without writing
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_research_ingest_plan_invalid_report_no_write --lib
    Targets: MCP invalid report handling and no-write checks.
  Given an invalid inline research report JSON object
  When calling `mempal_phase3` with `action=research_ingest_plan`
  Then the call succeeds with `research_ingest_plan.valid=false`
  And `research_ingest_plan.writes=false`
  And errors mention missing required fields
  And no drawer or runtime adoption event is created

Scenario: invalid phase3 action lists research ingest plan
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_rejects_invalid_action_without_mutation --lib
    Targets: MCP error message and mutation guard.
  Given an unsupported `mempal_phase3` action
  When the action is rejected
  Then the error lists `research_ingest_plan` among supported actions
  And no runtime adoption event is created

Scenario: MCP registry and protocol advertise research ingest plan
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
    Targets: MCP tool description and embedded memory protocol.
  Given the MCP tool registry and embedded memory protocol
  When inspecting the `mempal_phase3` surface
  Then both mention `research_ingest_plan`
  And both state that research ingest planning is read-only/advisory

Scenario: CLI dry-run keeps P67 behavior after sharing planner
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_dry_run_json_no_write
    Targets: CLI P67 regression guard.
  Given a valid research report file
  When running `mempal phase3 research-ingest-plan <path> --format json`
  Then the command still returns `writes=false`
  And no drawer is written

## Out of Scope

- MCP research ingestion execution.
- Research report fetching or browser automation.
- Research-driven promotion/demotion.
- Card embeddings or default card context changes.
