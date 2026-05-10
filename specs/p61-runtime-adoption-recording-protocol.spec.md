spec: task
name: "P61: runtime adoption recording protocol"
inherits: project
tags: ["phase-3", "runtime-adoption", "mcp", "protocol"]
---

## Intent

P60 made Phase-3 runtime evidence writable through MCP, but agents still need a
deterministic protocol for when to record `used`, `accepted`, `rejected`,
`miss`, `rollback`, `contradiction`, or `neutral`. P61 adds a read-only guidance
surface and protocol text so agents can record concrete runtime outcomes without
turning speculative reasoning into evidence.

## Decisions

- Extend `mempal_phase3` with `action=guidance`.
- `guidance` is read-only and must not write `runtime_adoption_events`, drawers,
  knowledge cards, or lifecycle events.
- Guidance version starts at `1`.
- Required record fields are `track`, `signal`, and `feature`.
- Guidance defines signal semantics for `used`, `accepted`, `rejected`, `miss`,
  `rollback`, `contradiction`, and `neutral`.
- Guidance defines track semantics and feature examples for `runtime_adoption`,
  `card_context`, `card_embedding`, `evaluator`, and `research_adapter`.
- `MEMORY_PROTOCOL` must tell agents to call `action=guidance` when unsure and
  to record only concrete runtime outcomes, not speculation.

## Boundaries

### Allowed Changes

- `src/mcp/server.rs`
- `src/mcp/tools.rs`
- `src/core/protocol.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-10-p61-runtime-adoption-recording-protocol.md`
- `specs/p61-runtime-adoption-recording-protocol.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden

- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change P56-P60 gates.
- Do not make card context default.
- Do not add card embeddings.
- Do not let evaluator or research guidance mutate lifecycle state.

## Acceptance Criteria

Scenario: guidance action returns recording protocol without side effects
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_guidance_action_is_read_only
  Level: unit
  Given an empty test database
  When `mempal_phase3` is called with `action=guidance`
  Then the response includes `version=1`
  And the response states that only concrete runtime outcomes should be recorded
  And the response includes required fields `track`, `signal`, and `feature`
  And the response defines `used` and `rollback` signal semantics
  And the response includes `card_context` with `include_cards` as a feature example
  And drawer and runtime adoption event counts remain unchanged

Scenario: protocol advertises guidance action and signal semantics
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface
  Level: unit
  Given the MCP tool registry and memory protocol
  When inspecting `mempal_phase3`
  Then the description lists `guidance/record/list/stats/gate/research_validate_plan`
  And `MEMORY_PROTOCOL` mentions `action=guidance`
  And `MEMORY_PROTOCOL` explains `used` and `rollback` recording semantics

## Out of Scope

- Automatic adoption recording after tool calls.
- Runtime hooks for Claude, Codex, or other agents.
- Adoption-event deduplication.
- Aggregated analytics beyond existing `stats`.
- Any default-on policy changes.
