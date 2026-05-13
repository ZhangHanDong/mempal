spec: task
name: "P79: rollback executor policy"
inherits: project
tags: [phase-3, rollback, context, cards]
---

## Intent

P78 added an explicit default runtime flag for card-aware context, but rollback
criteria were still mostly advisory: operators could disable the flag manually,
yet there was no deterministic executor that converted rollback evidence into a
policy action. P79 adds the smallest safe rollback executor for the existing
`card-context` default flag: evaluate runtime adoption evidence, report whether
rollback is required, and optionally disable the local config flag when
`--execute` is explicitly supplied.

## Decisions

- P79 must leave its own spec and plan per the P76 invariant.
- The only supported P79 candidate is `card-context`; it maps to
  `context.include_cards_default`.
- Rollback evidence is read from `runtime_adoption_events` with
  `track=card_context`, `feature=include_cards`, and `signal=rollback`.
- Rollback executor is safe-by-default: without `--execute`, it is read-only and
  must not write config or DB state.
- With `--execute`, rollback may only write local config and set
  `context.include_cards_default=false`; it must not append runtime adoption
  events or alter knowledge lifecycle state.
- If `context.include_cards_default` is already false, rollback execution is a
  no-op and must still report `applied=false`.
- The executor should be exposed through CLI `mempal phase3 rollback-control`
  and MCP `mempal_phase3 action=rollback_control`.
- Unknown candidates must fail before writing config.

## Boundaries

### Allowed Changes

- specs/p79-rollback-executor-policy.spec.md
- docs/plans/2026-05-13-p79-rollback-executor-policy.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/core/phase3.rs
- src/core/protocol.rs
- src/main.rs
- src/mcp/server.rs
- src/mcp/tools.rs
- tests/phase3_runtime.rs

### Forbidden

- Do not change SQLite schema.
- Do not create a generic rollback engine for unrelated candidates.
- Do not enable or disable card context automatically in background.
- Do not make `mempal context` include cards by default without config.
- Do not write runtime adoption events from rollback execution.
- Do not alter knowledge card lifecycle promotion or demotion rules.

## Acceptance Criteria

Scenario: CLI rollback check is read-only without execute
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_rollback_control_check_is_read_only
    Targets: CLI rollback-control read-only behavior.
  Given `context.include_cards_default=true`
  And rollback evidence exists for `card_context/include_cards`
  When running `mempal phase3 rollback-control card-context --format json`
  Then the command succeeds
  And JSON reports `writes=false`
  And JSON reports `rollback_required=true`
  And JSON reports `applied=false`
  And the config file still contains `include_cards_default = true`

Scenario: CLI rollback execute disables card context default
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_disables_default
    Targets: CLI rollback-control config side effect.
  Given `context.include_cards_default=true`
  And rollback evidence exists for `card_context/include_cards`
  When running `mempal phase3 rollback-control card-context --execute --format json`
  Then the command succeeds
  And JSON reports `writes=true`
  And JSON reports `rollback_required=true`
  And JSON reports `applied=true`
  And the config file contains `include_cards_default = false`

Scenario: CLI rollback execute is no-op without rollback evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_without_signal_is_noop
    Targets: CLI rollback-control no-signal behavior.
  Given `context.include_cards_default=true`
  And no rollback evidence exists for `card_context/include_cards`
  When running `mempal phase3 rollback-control card-context --execute --format json`
  Then the command succeeds
  And JSON reports `writes=false`
  And JSON reports `rollback_required=false`
  And JSON reports `applied=false`
  And the config file still contains `include_cards_default = true`

Scenario: CLI rollback execute is no-op when already disabled
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_already_disabled_is_noop
    Targets: CLI rollback-control disabled-state behavior.
  Given `context.include_cards_default=false`
  And rollback evidence exists for `card_context/include_cards`
  When running `mempal phase3 rollback-control card-context --execute --format json`
  Then the command succeeds
  And JSON reports `writes=false`
  And JSON reports `rollback_required=true`
  And JSON reports `applied=false`
  And the config file still contains `include_cards_default = false`

Scenario: CLI rejects unknown rollback candidate without config write
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_rollback_control_rejects_unknown_candidate_without_config_write
    Targets: CLI rollback-control error behavior.
  Given no config file exists
  When running `mempal phase3 rollback-control unknown --execute --format json`
  Then the command fails
  And stderr contains `unsupported phase3 rollback-control candidate`
  And no config file is created

Scenario: MCP rollback control check is read-only
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_rollback_control_check_is_read_only --lib
    Targets: MCP rollback-control read-only behavior.
  Given `mempal_phase3` receives action `rollback_control`
  And candidate `card-context`
  And execute is omitted
  And rollback evidence exists for `card_context/include_cards`
  When the tool returns a response
  Then `rollback_control.writes=false`
  And `rollback_control.rollback_required=true`
  And `rollback_control.applied=false`

Scenario: MCP tool registry documents rollback control
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
    Targets: MCP tool registry and memory protocol.
  Given the MCP tool registry is listed
  When inspecting `mempal_phase3`
  Then its description contains `rollback_control`
  And the memory protocol contains `action=rollback_control`

## Out of Scope

- Multi-candidate rollback policy.
- Background rollback daemon or hook.
- Automatic rollback based on live agent behavior without explicit `execute`.
- Runtime adoption event mutation, deletion, or synthesis.
