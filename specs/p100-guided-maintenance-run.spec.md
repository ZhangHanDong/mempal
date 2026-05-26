spec: task
name: "P100: guided maintenance run"
inherits: project
tags: [maintenance, guided-run, self-evolution, phase-5]
---

## Intent

P100 turns the static maintenance runbook into a deterministic guided dry-run.
The command should inspect current memory/runtime state and emit the next
recommended maintenance commands, while preserving the existing rule that
maintenance is not autonomous and does not grant promotion authority.

## Decisions

- Add CLI `mempal maintenance guided-run --format plain|json`.
- The guided run is read-only and returns `writes=false`.
- The report includes ordered steps for research validation, research ingest,
  knowledge distill, card lifecycle, context/adoption review, rollback review,
  cowork doctor, handoff, and cowork capture.
- The report includes current state counters when available: drawer count,
  runtime adoption event count, and card count.
- Guided run never executes generated commands.

## Boundaries

### Allowed Changes
- specs/p100-guided-maintenance-run.spec.md
- docs/plans/2026-05-26-p100-guided-maintenance-run.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/main.rs
- tests/ops_runtime.rs

### Forbidden
- Do not add a daemon, timer, hook, or background worker.
- Do not execute research, promotion, rollback, or capture commands.
- Do not mutate DB state.

## Acceptance Criteria

Scenario: CLI guided run JSON returns ordered commands
  Test:
    Filter: cargo test --test ops_runtime test_cli_maintenance_guided_run_json
    Targets: `mempal maintenance guided-run`.
  Given an initialized temp mempal DB
  When `mempal maintenance guided-run --format json` runs
  Then JSON reports `writes=false`
  And JSON includes steps for `research-validate-plan`
  And JSON includes steps for `adoption review`
  And JSON includes steps for `cowork-doctor`

Scenario: CLI guided run plain output is operator-readable
  Test:
    Filter: cargo test --test ops_runtime test_cli_maintenance_guided_run_plain
    Targets: plain report.
  When `mempal maintenance guided-run --format plain` runs
  Then stdout contains `Guided Maintenance Run`
  And stdout contains `mempal phase3 adoption review`
  And stdout contains `mempal cowork-capture`

Scenario: CLI guided run rejects unsupported format
  Test:
    Filter: cargo test --test ops_runtime test_cli_maintenance_guided_run_rejects_invalid_format
    Targets: format validation.
  When `mempal maintenance guided-run --format yaml` runs
  Then the command fails
  And stderr mentions `unsupported maintenance guided-run format`

## Out of Scope

- Automatic maintenance execution.
- Autonomous promotion/demotion.
- Scheduling.
