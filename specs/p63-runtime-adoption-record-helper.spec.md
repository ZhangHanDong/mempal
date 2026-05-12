spec: task
name: "P63: runtime adoption record helper"
inherits: project
tags: ["phase-3", "runtime-adoption", "cli", "mcp"]
---

## Intent

P61 and P62 expose when runtime adoption evidence should be recorded, but
agents still have to manually assemble the exact `record` parameters. P63 adds
a read-only helper that validates and normalizes candidate record inputs, then
returns the equivalent CLI command and MCP payload without appending an event.

## Decisions

- Add CLI `mempal phase3 adoption prepare-record`.
- Add MCP `mempal_phase3 action=prepare_record`.
- The helper validates `track`, `signal`, `feature`, and optional metadata JSON
  using the same parsing rules as `record`.
- The helper returns `writes=false`, a CLI `record_command`, and an MCP
  `record_payload`.
- The helper must not generate a runtime event id unless the caller explicitly
  supplied one.
- The helper is read-only and must not insert runtime adoption events.

## Boundaries

### Allowed Changes
- `src/core/phase3.rs`
- `src/main.rs`
- `src/mcp/**`
- `src/core/protocol.rs`
- `tests/phase3_runtime.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-12-p63-runtime-adoption-record-helper.md`
- `specs/p63-runtime-adoption-record-helper.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change Phase-3 gate thresholds.
- Do not make card context default.
- Do not add card embeddings.
- Do not create runtime adoption event ids for dry-run helper calls unless an
  explicit id was provided.

## Acceptance Criteria

Scenario: CLI prepare-record returns JSON without writing events
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_json_is_read_only
  Level: integration
  Given an empty CLI HOME
  And `skill trigger` is the query text
  When running `mempal phase3 adoption prepare-record --track card_context --signal accepted --feature include_cards --query "skill trigger" --format json`
  Then stdout is valid JSON
  And the response includes `writes=false`
  And `record_command` starts with `mempal phase3 adoption record`
  And `record_payload.action` is `record`
  And `record_payload.track` is `card_context`
  And runtime adoption event count remains zero

Scenario: CLI prepare-record supports plain output
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_plain
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption prepare-record --track card_context --signal used --feature include_cards`
  Then stdout mentions `writes=false`
  And stdout mentions `mempal phase3 adoption record`
  And stdout mentions `action=record`

Scenario: CLI prepare-record rejects invalid track
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_rejects_invalid_track
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption prepare-record --track invalid --signal accepted --feature x`
  Then the command fails
  And stderr mentions `unsupported runtime adoption track`

Scenario: MCP prepare_record is read-only
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_prepare_record_action_is_read_only
  Level: unit
  Given an empty test database
  When `mempal_phase3` is called with `action=prepare_record`
  Then the response includes `writes=false`
  And the response includes a `record_payload` with `action=record`
  And runtime adoption event count remains zero

## Out of Scope

- Automatic recording after helper output.
- Adoption-event deduplication.
- New analytics beyond existing `stats`.
- Any default-on policy changes.
