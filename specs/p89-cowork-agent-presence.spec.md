spec: task
name: "P89: cowork agent presence"
inherits: project
tags: [cowork, multi-agent, presence, heartbeat, phase-4]
---

## Intent

P89 adds explicit heartbeat-based presence to the multi-agent cowork bus.
Agents and operators should be able to distinguish registered-but-never-seen,
online, and stale agents without adding a daemon or automatic background
collector.

## Decisions

- Add `last_seen_at` to each bus agent registry record.
- `cowork-register` initializes `last_seen_at` to registration time.
- `cowork-heartbeat --agent-id <id>` updates `last_seen_at` and appends a
  heartbeat event.
- Presence states are derived at read time: `never_seen`, `online`, or `stale`.
- Default stale threshold is 10 minutes.
- CLI `cowork-agents` displays presence and last seen fields.
- MCP `mempal_cowork_bus action=list` returns presence and last seen fields,
  and action `heartbeat` updates one agent.
- Tests may pass explicit `--now` / `--seen-at` timestamps for deterministic
  stale calculations.

## Boundaries

### Allowed Changes
- specs/p89-cowork-agent-presence.spec.md
- docs/plans/2026-05-25-p89-cowork-agent-presence.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not add a daemon, watcher, hook, or silent heartbeat capture.
- Do not write `palace.db`, drawers, cards, runtime adoption events, or schema
  state.
- Do not infer presence from tmux pane state.
- Do not change legacy `mempal_cowork_push` or `mempal_peek_partner`
  behavior.

## Acceptance Criteria

Scenario: CLI heartbeat updates presence and last seen
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_heartbeat_updates_presence
    Targets: `src/main.rs` `cowork-heartbeat` and `cowork-agents`.
  Given registered `codex-a`
  And the CLI handler in `src/main.rs` is the exercised entry point
  When `mempal cowork-heartbeat --agent-id codex-a --seen-at 2026-05-25T00:00:00Z` runs
  And `mempal cowork-agents --now 2026-05-25T00:05:00Z` lists agents
  Then `codex-a` has `presence=online`
  And `last_seen_at=2026-05-25T00:00:00Z`

Scenario: CLI stale threshold marks old heartbeat stale
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_agents_marks_stale_presence
    Targets: presence replay in `cowork-agents`.
  Given registered `codex-a` last seen at `2026-05-25T00:00:00Z`
  When `cowork-agents --now 2026-05-25T00:11:00Z` runs
  Then `codex-a` has `presence=stale`

Scenario: CLI heartbeat rejects unknown agent
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_heartbeat_rejects_unknown_agent
    Targets: heartbeat validation.
  Given no registered `codex-missing`
  When `cowork-heartbeat --agent-id codex-missing` runs
  Then the command fails
  And stderr contains `unknown agent`

Scenario: MCP heartbeat and list expose presence
  Test:
    Filter: test_mcp_cowork_bus_heartbeat_and_presence
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=heartbeat|list`.
  Given registered `codex-a` through MCP
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  When `action=heartbeat` updates `codex-a`
  Then `action=list` returns `presence=online`
  And no `palace.db` file is created

## Out of Scope

- Automatic heartbeat from shell hooks.
- tmux pane liveness detection.
- Cross-machine presence.
- Scheduling or cleanup of stale agents.
- Delivery ack/status changes.
