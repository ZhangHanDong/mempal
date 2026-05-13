spec: task
name: "P72: runtime adoption capture helper"
inherits: project
tags: [phase-3, self-evolution, runtime-adoption, capture]
---

## Intent

P71 proved the self-evolution pieces can be composed, but adoption evidence is
still too manual: an agent must know the exact `track/signal/feature` tuple for
every runtime outcome. P72 adds an explicit capture helper that maps a small
surface/outcome vocabulary into the existing P69 checked-record path, lowering
recording friction without adding background hooks or autonomous capture.

## Decisions

- P72 must leave its own `specs/p72-*.spec.md` and plan document.
- Add CLI `mempal phase3 adoption capture`.
- Add MCP `mempal_phase3 action=capture`.
- Capture inputs use `surface` and `outcome`; the helper maps them to existing
  `track`, `signal`, and `feature` values.
- Capture defaults to dry-run/read-only and returns a record plan plus quality
  report.
- Capture writes only when `--execute` / `execute=true` is set, and writes must
  reuse the P69 checked-record policy.
- Warning-quality captures remain blocked by default and require
  `--allow-warnings` / `allow_warnings=true` to write.

## Boundaries

### Allowed Changes
- specs/p72-runtime-adoption-capture-helper.spec.md
- docs/plans/2026-05-13-p72-runtime-adoption-capture-helper.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/core/phase3.rs
- src/main.rs
- src/mcp/server.rs
- src/mcp/tools.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add schema migrations.
- Do not add background hooks or automatic instrumentation.
- Do not bypass `check_record` / `record_checked` quality policy.
- Do not change existing `record`, `prepare-record`, `check-record`, or
  `record-checked` behavior.
- Do not mark the overall self-evolution objective complete.

## Acceptance Criteria

Scenario: CLI capture dry-run maps card context outcome through src/main.rs without writing
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_capture_card_context_dry_run
    Targets: CLI capture dry-run behavior.
  Given a card-context accepted outcome with query "skill trigger" and note "card helped"
  When running `mempal phase3 adoption capture --surface card-context --outcome accepted --card-id card_1 --query "skill trigger" --note "card helped" --format json`
  Then stdout contains `writes=false`
  And stdout contains a record plan with `track=card_context`, `signal=accepted`, and `feature=include_cards`
  And the quality report is `ready`
  And no runtime adoption event is persisted

Scenario: CLI capture execute writes through checked policy
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_capture_execute_writes_ready_event
    Targets: CLI capture execute behavior.
  Given a ready card-context accepted outcome
  When running capture with `--execute --format json`
  Then stdout contains `writes=true`
  And exactly one runtime adoption event is persisted

Scenario: CLI capture blocks warning by default
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_capture_blocks_warning_by_default
    Targets: CLI capture warning policy.
  Given a card-context accepted outcome without `card_id`
  When running capture with `--execute --format json`
  Then stdout contains `writes=false` and `blocked=true`
  And no runtime adoption event is persisted

Scenario: MCP capture supports dry-run and execute through src/mcp/server.rs
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_capture_action_dry_run_and_execute --lib
    Targets: MCP capture action in `src/mcp/server.rs`.
  Given `mempal_phase3 action=capture`
  When called once without `execute` and once with `execute=true`
  Then the dry-run response has no DB side effect
  And the execute response writes through checked-record policy

Scenario: Capture rejects unknown surface
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_capture_rejects_unknown_surface
    Targets: CLI capture input validation in `src/main.rs`.
  Given an unsupported capture surface
  When running capture
  Then the command fails
  And stderr mentions `unsupported adoption capture surface`

Scenario: Inventories include P72
  Test:
    Filter: rg -n "p72-runtime-adoption-capture-helper|P72 runtime adoption capture helper" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Agent inventories and design document.
  Given project documentation
  When searching for P72
  Then the spec, plan, and design status are recorded

## Out of Scope

- Automatic capture from live agent tool calls.
- Hook installation or shell/TUI instrumentation.
- New runtime adoption tables or schema changes.
- Evaluator advisory API.
- Default-on card context.
