spec: task
name: "P96: cowork memory capture"
inherits: project
tags: [cowork, memory, evidence, mcp, phase-4]
---

## Intent

P96 adds an explicit bridge from ephemeral cowork runtime state to durable
memory. Agents can preview or execute a capture of the deterministic P95
handoff summary into an evidence drawer, but mempal still never silently stores
runtime chat or terminal content.

## Decisions

- Add CLI `mempal cowork-capture --cwd <repo>`.
- Add MCP action `mempal_cowork_bus action=capture`.
- Default mode is dry-run; writes require explicit `--execute` or
  `execute=true`.
- Only `--summary-source handoff` is supported in P96.
- Execute writes one evidence drawer using wing `cowork-capture` by default.
- Capture does not write vectors, does not promote knowledge, and does not
  alter cowork events or delivery status.

## Boundaries

### Allowed Changes
- specs/p96-cowork-memory-capture.spec.md
- docs/plans/2026-05-26-p96-cowork-memory-capture.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/cowork/mod.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not add automatic capture hooks.
- Do not ingest raw terminal pane content from `tmux_peek`.
- Do not bypass evidence drawer governance.
- Do not create knowledge cards or promote knowledge.

## Acceptance Criteria

Scenario: CLI capture dry-run previews drawer without writing
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_capture_dry_run_no_write
    Targets: `cowork-capture` dry-run behavior.
  Given a handoff summary exists
  When `mempal cowork-capture --summary-source handoff --format json` runs
  Then JSON reports `writes=false`
  And no `palace.db` file is created

Scenario: CLI capture execute writes an evidence drawer
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_capture_execute_writes_evidence
    Targets: evidence drawer write path.
  Given a handoff summary exists
  When `cowork-capture --execute --format json` runs
  Then JSON reports `writes=true`
  And JSON includes a `drawer_id`
  And loading that `drawer_id` returns an evidence drawer in wing `cowork-capture`
  And the drawer content contains the captured handoff

Scenario: CLI capture rejects unsupported source
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_capture_rejects_unknown_source
    Targets: source validation.
  When `cowork-capture --summary-source tmux` runs
  Then the command fails
  And stderr mentions `unsupported cowork capture summary source`

Scenario: MCP capture supports dry-run and execute
  Test:
    Filter: test_mcp_cowork_bus_capture
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=capture`.
  Given an MCP capture request with `execute=false`
  Then no `palace.db` file is created
  When the same request uses `execute=true`
  Then the response includes a drawer id

## Out of Scope

- Automatic capture.
- Capturing raw tmux pane text.
- Knowledge card creation.
- Vector embedding for captured evidence.
