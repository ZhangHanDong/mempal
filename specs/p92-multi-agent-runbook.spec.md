spec: task
name: "P92: multi-agent cowork runbook"
inherits: project
tags: [cowork, multi-agent, runbook, phase-4]
---

## Intent

P92 turns the P84-P91 multi-agent runtime surfaces into an operator-facing
runbook. The goal is not to add a new communication primitive; it is to make
register, send, broadcast, drain, ack, presence, channels, threads, tmux send,
and tmux peek usable as one deterministic workflow.

## Decisions

- Add authoritative documentation at `docs/COWORK-RUNBOOK.md`.
- Add read-only CLI `mempal cowork-runbook --format plain|json`.
- The CLI prints bundled static runbook content and never reads or writes
  `~/.mempal`, bus state, or `palace.db`.
- JSON output includes the runbook title and content as machine-readable fields.
- P92 does not add MCP actions; MCP users already have direct bus actions.

## Boundaries

### Allowed Changes
- specs/p92-multi-agent-runbook.spec.md
- docs/plans/2026-05-26-p92-multi-agent-runbook.md
- docs/COWORK-RUNBOOK.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/main.rs
- tests/cowork_bus.rs

### Forbidden
- Do not change cowork bus message delivery behavior.
- Do not write runtime bus files from `cowork-runbook`.
- Do not write `palace.db` from `cowork-runbook`.

## Acceptance Criteria

Scenario: CLI runbook plain output covers the multi-agent workflow
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_runbook_plain
    Targets: `src/main.rs` `cowork-runbook`.
  When `mempal cowork-runbook --format plain` runs
  Then stdout contains `cowork-register`
  And stdout contains `cowork-channel-send`
  And stdout contains `cowork-tmux-peek`
  And stderr is empty

Scenario: CLI runbook JSON output is machine readable
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_runbook_json
    Targets: `src/main.rs` JSON output for `cowork-runbook`.
  When `mempal cowork-runbook --format json` runs
  Then stdout is valid JSON
  And JSON field `title` contains `Multi-Agent Cowork Runbook`
  And JSON field `content` contains `cowork-deliveries`

Scenario: CLI runbook rejects unsupported format without side effects
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_runbook_rejects_invalid_format
    Targets: `src/main.rs` format validation.
  Given a clean fake HOME
  When `mempal cowork-runbook --format yaml` runs
  Then the command fails
  And no `.mempal` directory is created

Scenario: Documentation inventory references P92
  Test:
    Filter: rg -n "p92-multi-agent-runbook|P92 multi-agent cowork runbook|COWORK-RUNBOOK" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: inventory and design documentation.
  When the inventory files are inspected
  Then they mention the P92 spec
  And they mention the matching P92 plan
  And the design document mentions `COWORK-RUNBOOK`

## Out of Scope

- New cowork transport modes.
- New MCP actions.
- Automatic agent registration.
- Persisting runbook reads as adoption evidence.
