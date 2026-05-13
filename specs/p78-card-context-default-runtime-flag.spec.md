spec: task
name: "P78: card context default runtime flag"
inherits: project
tags: [phase-3, context, cards, default-policy]
---

## Intent

P74 introduced a read-only default-on proposal for card-aware context, and P75
left the actual default-on runtime change as an open gap. P78 adds an explicit,
reversible runtime flag for card context defaults. The flag must remain disabled
by default, must only be enabled through a proposal-ready Phase-3 control path,
and must be overrideable per request.

## Decisions

- Add config field `context.include_cards_default`, defaulting to `false`.
- `mempal context` uses `context.include_cards_default` when neither
  `--include-cards` nor `--no-include-cards` is provided.
- `mempal_context` uses `context.include_cards_default` when MCP
  `include_cards` is omitted.
- Add CLI `mempal phase3 default-control card-context`.
- `default-control --enable` must require a P74-ready proposal: sufficient
  card-context readiness plus at least one rollback criterion.
- `default-control --disable` is always allowed and writes the flag back to
  `false`.
- The control command writes only the local mempal config file, not drawers,
  runtime adoption events, cards, or schema.
- P78 must leave its own spec and plan per the P76 invariant.

## Boundaries

### Allowed Changes
- specs/p78-card-context-default-runtime-flag.spec.md
- docs/plans/2026-05-13-p78-card-context-default-runtime-flag.md
- src/core/config.rs
- src/main.rs
- src/mcp/server.rs
- src/core/protocol.rs
- tests/context_assembler.rs
- tests/phase3_runtime.rs
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md

### Forbidden
- Do not change search defaults.
- Do not make card context default-on without config.
- Do not enable the flag when P74 proposal readiness fails.
- Do not add new database schema.
- Do not append runtime adoption events from `default-control`.
- Do not remove explicit `--include-cards` or MCP `include_cards=true`.

## Acceptance Criteria

Scenario: Config default keeps card context opt-in
  Test:
    Filter: cargo test --test context_assembler test_cli_context_config_default_false_omits_cards
    Targets: CLI context default behavior.
  Given no config file or a config with `context.include_cards_default=false`
  And query text `card-aware`
  When running `mempal context "card-aware" --format json` without card flags
  Then the context response omits card sections

Scenario: Config flag enables card context by default
  Test:
    Filter: cargo test --test context_assembler test_cli_context_config_default_true_includes_cards
    Targets: CLI context config default.
  Given a config with `context.include_cards_default=true`
  And an active knowledge card with linked evidence exists
  And query text `card-aware`
  When running `mempal context "card-aware" --format json` without card flags
  Then the context response includes card sections

Scenario: CLI per-request no flag overrides config default
  Test:
    Filter: cargo test --test context_assembler test_cli_context_no_include_cards_overrides_config_default_true
    Targets: CLI override behavior.
  Given a config with `context.include_cards_default=true`
  And query text `card-aware`
  When running `mempal context "card-aware" --no-include-cards --format json`
  Then the context response omits card sections

Scenario: MCP omitted include_cards follows config default
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_context_include_cards_omitted_uses_config_default --lib
    Targets: MCP context behavior.
  Given an MCP server configured with `context.include_cards_default=true`
  And a request omits `include_cards`
  When calling `mempal_context`
  Then the response includes card sections

Scenario: Default control enables only after proposal is ready
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_control_enable_requires_ready_proposal
    Targets: CLI default-control gate and config file side effect.
  Given no accepted `card_context/include_cards` evidence
  And rollback criterion text `disable if rollbacks appear`
  When running `mempal phase3 default-control card-context --enable --rollback-criterion "disable if rollbacks appear" --format json`
  Then the command reports `applied=false`
  And `context.include_cards_default` remains false
  Given three accepted `card_context/include_cards` events
  When running the same enable command
  Then it reports `applied=true`
  And config `context.include_cards_default` becomes true

Scenario: Default control writes config file on successful enable
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_control_enable_writes_config_file_output
    Targets: Config file side effect.
  Given three accepted `card_context/include_cards` events
  And a rollback criterion value is provided
  When running `mempal phase3 default-control card-context --enable --rollback-criterion "disable if rollbacks appear" --format json`
  Then the local config file exists
  And the file contains `include_cards_default = true`

Scenario: Default control rejects unknown candidate without config write
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_control_rejects_unknown_candidate_without_config_write
    Targets: CLI error path and config file side effect.
  Given no config file exists
  When running `mempal phase3 default-control unknown --enable --rollback-criterion "disable if rollbacks appear" --format json`
  Then the command fails
  And stderr includes `unsupported phase3 default-control candidate`
  And no config file is created

Scenario: Default control disable is reversible and read-model safe
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_control_disable_is_reversible
    Targets: CLI default-control disable path.
  Given config `context.include_cards_default=true`
  When running `mempal phase3 default-control card-context --disable --format json`
  Then it reports `applied=true`
  And config `context.include_cards_default` becomes false
  And no runtime adoption event is appended

## Out of Scope

- Implementing rollback executor policy.
- Installing live instrumentation hooks.
- Making search card-aware by default.
- Adding new persistence schema.
