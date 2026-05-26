spec: task
name: "P93: cowork doctor"
inherits: project
tags: [cowork, multi-agent, diagnostics, mcp, phase-4]
---

## Intent

P93 adds a deterministic diagnostic surface for the multi-agent cowork bus.
Operators and agents should be able to inspect registry health, stale
presence, pending deliveries, channel/session state, and optional tmux target
reachability without mutating runtime state or memory.

## Decisions

- Add CLI `mempal cowork-doctor --cwd <repo> [--now RFC3339] [--probe-tmux]`.
- Add MCP action `mempal_cowork_bus action=doctor`.
- Default output is plain; `--format json` returns a stable diagnostic object.
- `--probe-tmux` uses direct `std::process::Command` invocation of
  `tmux has-session -t <target>` and never executes through a shell.
- Without `--probe-tmux`, tmux agents are reported as `not_probed`.
- Doctor is read-only: it does not append events, drain inboxes, update
  heartbeat, modify sessions, or write `palace.db`.

## Boundaries

### Allowed Changes
- specs/p93-cowork-doctor.spec.md
- docs/plans/2026-05-26-p93-cowork-doctor.md
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
- Do not add background daemons, watchers, hooks, or automatic heartbeats.
- Do not write bus events from doctor.
- Do not fall back to shell execution for tmux probes.

## Acceptance Criteria

Scenario: CLI doctor reports empty registry warning
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_doctor_empty_registry
    Targets: `src/main.rs` `cowork-doctor`.
  Given no registered agents for a project
  When `mempal cowork-doctor --cwd <repo>` runs
  Then stdout contains `status=warning`
  And stdout contains `no registered agents`
  And no `events.jsonl` file is created

Scenario: CLI doctor reports stale agents and pending deliveries
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_doctor_reports_stale_and_pending
    Targets: doctor status derivation from presence and deliveries.
  Given one stale agent and one pending delivery
  When `cowork-doctor --now 2026-05-25T00:20:00Z` runs
  Then stdout contains `stale_agents=1`
  And stdout contains `pending_deliveries=1`
  And stdout contains `status=warning`

Scenario: CLI doctor JSON includes tmux probe result
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_doctor_json_tmux_probe
    Targets: `--format json` and direct tmux probe behavior.
  Given fake `tmux` first in PATH
  And a registered tmux agent
  When `cowork-doctor --probe-tmux --format json` runs
  Then stdout is valid JSON
  And JSON includes `tmux.ok`
  And the fake tmux log records `has-session`, `-t`, and the registered target

Scenario: MCP doctor mirrors CLI diagnostics
  Test:
    Filter: test_mcp_cowork_bus_doctor
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=doctor`.
  Given registered agents and pending delivery state
  When MCP action `doctor` runs
  Then the response includes a doctor payload
  And no `palace.db` file is created

## Out of Scope

- Auto-repair.
- Killing or restarting tmux panes.
- Sending reminder messages.
- Persisting diagnostic reports as memory.
