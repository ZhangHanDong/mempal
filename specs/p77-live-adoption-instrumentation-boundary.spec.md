spec: task
name: "P77: live adoption instrumentation boundary"
inherits: project
tags: [phase-3, runtime-adoption, instrumentation, policy]
---

## Intent

P75 identified missing automatic live tool instrumentation as a remaining gap in
the self-evolution objective. P77 defines the safe boundary before any hooks or
tool wrappers are allowed: instrumentation may help prepare or trigger checked
adoption capture, but it must not silently append evidence or bypass existing
quality gates. The boundary must be exposed to both CLI users and MCP agents so
future runtime wrappers can follow the same contract.

## Decisions

- Add CLI `mempal phase3 adoption instrumentation-policy`.
- Add MCP `mempal_phase3 action=instrumentation_policy`.
- The policy surface is read-only and must not append `runtime_adoption_events`.
- The default instrumentation mode remains `manual_only`.
- `opt_in_wrapper` is the only semi-automatic mode allowed by P77.
- `implicit_background_capture` is explicitly forbidden.
- Any wrapper-created capture must go through existing `capture` /
  `record_checked` quality gates and must preserve opt-out and rollback
  requirements.
- P77 must leave its own spec and plan per the P76 invariant.

## Boundaries

### Allowed Changes
- specs/p77-live-adoption-instrumentation-boundary.spec.md
- docs/plans/2026-05-13-p77-live-adoption-instrumentation-boundary.md
- src/core/phase3.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- src/core/protocol.rs
- tests/phase3_runtime.rs
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md

### Forbidden
- Do not add actual shell hooks, background workers, daemon processes, or tool
  wrappers.
- Do not change `capture` default dry-run behavior.
- Do not write runtime adoption events from `instrumentation-policy`.
- Do not make `include_cards` default-on.
- Do not add new database schema or external dependencies.

## Acceptance Criteria

Scenario: CLI exposes read-only instrumentation policy
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_instrumentation_policy_json_is_read_only
    Targets: CLI policy output and event count.
  Given an empty runtime adoption event table
  When running `mempal phase3 adoption instrumentation-policy --format json`
  Then the JSON report has `writes=false`
  And `default_mode` is `manual_only`
  And allowed modes include `opt_in_wrapper`
  And forbidden modes include `implicit_background_capture`
  And the runtime adoption event count remains zero

Scenario: CLI rejects unsupported instrumentation policy output format
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_instrumentation_policy_rejects_invalid_format
    Targets: CLI error handling.
  Given the CLI policy command
  When running it with `--format yaml`
  Then the command fails
  And stderr includes `unsupported phase3 adoption format`

Scenario: MCP exposes read-only instrumentation policy
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_instrumentation_policy_action_is_read_only --lib
    Targets: MCP policy action in `src/mcp/server.rs`.
  Given an empty test database
  When `mempal_phase3` is called with `action=instrumentation_policy`
  Then the response includes an instrumentation policy with `writes=false`
  And one allowed mode is `opt_in_wrapper`
  And no runtime adoption event is appended
  And the behavior is exercised through `src/mcp/server.rs`

Scenario: Protocol advertises instrumentation boundary
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
    Targets: MCP tool description and MEMORY_PROTOCOL.
  Given the MCP tool registry and protocol instructions
  When inspecting the Phase-3 surface
  Then `instrumentation_policy` is listed as a supported action
  And the protocol states that live instrumentation is opt-in and must use checked capture

Scenario: Inventories include P77
  Test:
    Filter: rg -n "p77-live-adoption-instrumentation-boundary|P77 live adoption instrumentation boundary" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Project inventories and design document.
  Given project inventories and MIND-MODEL design
  When searching for P77
  Then the P77 spec, plan, and design summary are recorded

## Out of Scope

- Installing runtime hooks.
- Wrapping agent tool calls.
- Executing rollback policies.
- Automatically changing context defaults.
