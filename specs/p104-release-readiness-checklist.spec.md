spec: task
name: "P104: release readiness checklist"
inherits: project
tags: [release, readiness, checklist, phase-5]
---

## Intent

P104 adds a deterministic release readiness checklist for real-world use. After
P82-P103 the project has many runtime surfaces; release operators need a single
read-only command that checks documentation, specs/plans, install diagnostics,
and package metadata before publishing or recommending an install.

## Decisions

- Add CLI `mempal release-readiness --format plain|json`.
- The checklist reports `ready`, `writes=false`, individual checks, warnings,
  and recommended commands.
- Checks include Cargo package metadata, README presence, P98-P104 spec/plan
  inventory, runbooks, doctor availability, and current DB schema support.
- The command is read-only and does not run `cargo package`.
- P104 updates AGENTS / CLAUDE inventory through P104.

## Boundaries

### Allowed Changes
- specs/p104-release-readiness-checklist.spec.md
- docs/plans/2026-05-26-p104-release-readiness-checklist.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/main.rs
- tests/ops_runtime.rs

### Forbidden
- Do not publish to crates.io.
- Do not run network operations.
- Do not create or modify DB state.

## Acceptance Criteria

Scenario: CLI release readiness JSON reports required checks
  Test:
    Filter: cargo test --test ops_runtime test_cli_release_readiness_json
    Targets: `release-readiness`.
  When `mempal release-readiness --format json` runs from the repo root
  Then JSON reports `writes=false`
  And includes checks for `cargo-metadata`
  And includes checks for `spec-plan-inventory`
  And includes recommended verification commands

Scenario: CLI release readiness plain output is actionable
  Test:
    Filter: cargo test --test ops_runtime test_cli_release_readiness_plain
    Targets: plain output.
  When `mempal release-readiness --format plain` runs
  Then stdout contains `Release Readiness`
  And stdout contains `cargo package`
  And stdout contains `mempal doctor`

Scenario: CLI release readiness rejects unsupported format
  Test:
    Filter: cargo test --test ops_runtime test_cli_release_readiness_rejects_invalid_format
    Targets: format validation.
  When `mempal release-readiness --format yaml` runs
  Then the command fails
  And stderr mentions `unsupported release-readiness format`

## Out of Scope

- Publishing.
- GitHub release creation.
- Running package builds automatically.
