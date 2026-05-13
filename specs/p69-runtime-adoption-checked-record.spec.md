spec: task
name: "P69: runtime adoption checked record"
inherits: project
tags: [phase-3, runtime-adoption, quality, cli, mcp]
---

## Intent

P63 can prepare runtime adoption record inputs and P64 can evaluate their
quality, but agents still have to manually run `check_record` before `record`.
P69 adds a guarded write path that evaluates the same quality policy immediately
before appending a runtime adoption event, so self-evolution evidence capture is
safer by default without adding automatic hooks or changing Phase-3 gates.

## Decisions

- Add CLI `mempal phase3 adoption record-checked`.
- Add MCP `mempal_phase3 action=record_checked`.
- `record_checked` reuses the same input fields as `record`.
- `record_checked` runs the P64 quality policy before any write.
- By default, only `quality=ready` records are written.
- `--allow-warnings` / `allow_warnings=true` permits `quality=warning` records
  to be written, but `quality=invalid` is always blocked.
- The response reports `writes`, `blocked`, the quality report, and the optional
  written event.
- Existing `record`, `check_record`, and gate semantics remain unchanged.

## Boundaries

### Allowed Changes
- specs/p69-runtime-adoption-checked-record.spec.md
- docs/plans/2026-05-13-p69-runtime-adoption-checked-record.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/core/phase3.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not automatically record events after context/search/research tool calls.
- Do not change schema v9.
- Do not change Phase-3 gate thresholds.
- Do not make card context default.
- Do not add card embeddings.
- Do not change existing `record` behavior.
- Do not let `allow_warnings` write invalid records.

## Acceptance Criteria

Scenario: CLI checked record writes ready evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_writes_ready_event
    Targets: CLI checked record happy path and DB write.
  Given an empty CLI HOME
  When running `mempal phase3 adoption record-checked --track runtime_adoption --signal accepted --feature context_pack --query "skill trigger" --note "context guidance helped" --format json`
  Then stdout is valid JSON
  And `writes=true`
  And `blocked=false`
  And `record_quality.quality` is `ready`
  And a runtime adoption event is written

Scenario: CLI checked record blocks warning evidence by default
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_blocks_warning_by_default
    Targets: CLI warning block and no-write side effect check.
  Given an empty CLI HOME
  When running `mempal phase3 adoption record-checked --track card_context --signal accepted --feature include_cards --format json`
  Then stdout is valid JSON
  And `writes=false`
  And `blocked=true`
  And `record_quality.quality` is `warning`
  And no runtime adoption event is written

Scenario: CLI checked record allow-warnings writes warning evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_allow_warnings_writes_warning_event
    Targets: CLI allow-warnings behavior and DB write.
  Given an empty CLI HOME
  When running `mempal phase3 adoption record-checked --track card_context --signal accepted --feature include_cards --allow-warnings --format json`
  Then stdout is valid JSON
  And `writes=true`
  And `blocked=false`
  And `record_quality.quality` is `warning`
  And a runtime adoption event is written

Scenario: CLI checked record always blocks invalid evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_blocks_invalid_even_with_allow_warnings
    Targets: CLI invalid block and no-write side effect check.
  Given an empty CLI HOME
  When running `mempal phase3 adoption record-checked --track card_context --signal accepted --feature "   " --allow-warnings --format json`
  Then stdout is valid JSON
  And `writes=false`
  And `blocked=true`
  And `record_quality.quality` is `invalid`
  And no runtime adoption event is written

Scenario: MCP checked record is quality-gated
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_record_checked_quality_gated --lib
    Targets: MCP checked record write/block behavior and DB side effects.
  Given an empty test database
  When `mempal_phase3` is called with `action=record_checked` and a ready record
  Then the response includes `writes=true`
  And one runtime adoption event is appended
  When `mempal_phase3` is called with warning-quality input without `allow_warnings`
  Then the response includes `writes=false`
  And no additional event is appended

Scenario: MCP registry and protocol advertise checked record
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
    Targets: MCP tool description and embedded memory protocol.
  Given the MCP tool registry and embedded memory protocol
  When inspecting the `mempal_phase3` surface
  Then both mention `record_checked`
  And the protocol states it is quality-gated.

## Out of Scope

- Automatic adoption recording after tool calls.
- Runtime hooks for Claude, Codex, or other agents.
- Event deduplication.
- New schema, indexes, or gate thresholds.
- Any default-on policy changes.
