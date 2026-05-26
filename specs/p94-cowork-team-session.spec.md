spec: task
name: "P94: cowork team session"
inherits: project
tags: [cowork, multi-agent, session, mcp, phase-4]
---

## Intent

P94 adds an explicit team-session runtime object above agents, channels, and
threads. A session records the current collaboration goal and membership for a
project without becoming durable memory and without changing message delivery.

## Decisions

- Store sessions in `~/.mempal/cowork-bus/<project>/sessions.json`.
- Add CLI `cowork-session-create`, `cowork-sessions`, and
  `cowork-session-status`.
- Add MCP actions `session_create`, `session_list`, and `session_status` to
  `mempal_cowork_bus`.
- Session ids use the same safe token rule as agent ids.
- Session status is one of `active`, `paused`, or `closed`.
- Session changes append operational bus events but do not write `palace.db`.

## Boundaries

### Allowed Changes
- specs/p94-cowork-team-session.spec.md
- docs/plans/2026-05-26-p94-cowork-team-session.md
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
- Do not migrate SQLite schema.
- Do not alter send/broadcast/channel delivery semantics.
- Do not auto-create agents or channels.

## Acceptance Criteria

Scenario: CLI creates and lists a team session
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_session_create_and_list
    Targets: `cowork-session-create` and `cowork-sessions`.
  Given registered `claude-main` and `codex-a`
  When `cowork-session-create --session-id review-1 --agent claude-main --agent codex-a` runs
  Then `cowork-sessions --cwd <repo>` prints `review-1`
  And `sessions.json` exists under the project bus directory

Scenario: CLI rejects sessions with unknown agents
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_session_rejects_unknown_agent
    Targets: session membership validation.
  Given only `claude-main` is registered
  When creating a session with `codex-missing`
  Then the command fails
  And `sessions.json` is not created

Scenario: CLI updates session status
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_session_status_update
    Targets: `cowork-session-status`.
  Given an active session
  When `cowork-session-status --session-id review-1 --status paused` runs
  Then `cowork-sessions --format json` reports status `paused`
  And `cowork-events` includes `session_status`

Scenario: MCP creates and lists sessions
  Test:
    Filter: test_mcp_cowork_bus_sessions
    Targets: `src/mcp/server.rs` session actions.
  Given registered agents
  When MCP action `session_create` runs
  Then MCP action `session_list` returns the created session
  And no `palace.db` file is created

## Out of Scope

- Durable project memory for sessions.
- Scheduling.
- Assignment state machines.
- Cross-project sessions.
