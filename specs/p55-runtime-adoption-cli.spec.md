spec: task
name: "P55: runtime adoption CLI"
inherits: project
tags: [phase-3, runtime-adoption, cli]
---

## Intent

P55 exposes the P54 evidence substrate through a minimal CLI so agents and
hooks can explicitly record, inspect, and summarize runtime adoption events.

## Decisions

- Add `mempal phase3 adoption record`.
- Add `mempal phase3 adoption list`.
- Add `mempal phase3 adoption stats`.
- CLI supports `plain` and `json` output where relevant.
- Invalid tracks or signals are rejected by parsing.

## Boundaries

### Allowed Changes
- src/main.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not auto-record events from context/search by default.
- Do not write lifecycle status from adoption events.

## Acceptance Criteria

Scenario: CLI records and summarizes adoption events
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_stats_and_gate
  Given an empty mempal home
  When recording card context adoption events
  Then stats report accepted events
  And gate evaluation can consume those events

Scenario: invalid track is rejected
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_rejects_invalid_track
  Given the CLI
  When an unsupported track is supplied
  Then the command exits with an error

## Out of Scope

- Background telemetry.
- Network reporting.
