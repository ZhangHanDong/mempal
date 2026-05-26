spec: task
name: "P97: maintenance runbook"
inherits: project
tags: [self-evolution, auto-dream, maintenance, phase-4]
---

## Intent

P97 documents the operational maintenance loop that connects research,
evidence, distillation, card governance, context adoption, runtime adoption
evidence, cowork handoff, and explicit cowork capture. It gives humans and
agents a deterministic checklist for dream-cycle style maintenance without
introducing autonomous background behavior.

## Decisions

- Add authoritative documentation at `docs/MAINTENANCE-RUNBOOK.md`.
- Add read-only CLI `mempal maintenance-runbook --format plain|json`.
- The runbook must mention research ingest, knowledge distill, card lifecycle,
  context adoption review, cowork handoff, and explicit cowork capture.
- The CLI is read-only and does not run maintenance commands.
- P97 does not add daemons, timers, hooks, or autonomous promotion.

## Boundaries

### Allowed Changes
- specs/p97-maintenance-runbook.spec.md
- docs/plans/2026-05-26-p97-maintenance-runbook.md
- docs/MAINTENANCE-RUNBOOK.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/main.rs
- tests/cowork_bus.rs

### Forbidden
- Do not add background auto-dream execution.
- Do not run research, ingest, distill, promote, demote, or capture from the
  runbook command.
- Do not change phase3 lifecycle authority.

## Acceptance Criteria

Scenario: CLI maintenance runbook plain output covers the governed loop
  Test:
    Filter: cargo test --test cowork_bus test_cli_maintenance_runbook_plain
    Targets: `src/main.rs` `maintenance-runbook`.
  When `mempal maintenance-runbook --format plain` runs
  Then stdout contains `research-ingest-plan`
  And stdout contains `knowledge distill`
  And stdout contains `cowork-capture`

Scenario: CLI maintenance runbook JSON output is machine readable
  Test:
    Filter: cargo test --test cowork_bus test_cli_maintenance_runbook_json
    Targets: JSON output for `maintenance-runbook`.
  When `mempal maintenance-runbook --format json` runs
  Then stdout is valid JSON
  And JSON field `title` contains `Maintenance Runbook`
  And JSON field `content` contains `runtime adoption`

Scenario: CLI maintenance runbook rejects unsupported format without side effects
  Test:
    Filter: cargo test --test cowork_bus test_cli_maintenance_runbook_rejects_invalid_format
    Targets: format validation and read-only boundary.
  Given a clean fake HOME
  When `mempal maintenance-runbook --format yaml` runs
  Then the command fails
  And no `.mempal` directory is created

Scenario: Documentation inventory references P97
  Test:
    Filter: rg -n "p97-maintenance-runbook|P97 maintenance runbook|MAINTENANCE-RUNBOOK" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: inventory and design documentation.
  When the inventory files are inspected
  Then they mention the P97 spec
  And they mention the matching P97 plan
  And the design document mentions `MAINTENANCE-RUNBOOK`

## Out of Scope

- Autonomous maintenance execution.
- Scheduling.
- Silent capture.
- Lifecycle promotion without human-gated policy.
