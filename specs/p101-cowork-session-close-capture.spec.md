spec: task
name: "P101: cowork session close capture"
inherits: project
tags: [cowork, session, handoff, capture, mcp, phase-5]
---

## Intent

P101 reduces multi-agent shutdown friction by adding an explicit session close
flow. Closing a session updates runtime session state and can optionally bridge
the deterministic handoff summary into durable evidence through the existing
P96 capture path.

## Decisions

- Add CLI `mempal cowork-session-close --cwd <repo> --session-id <id>`.
- Add optional `--capture` and `--execute` flags; capture defaults to dry-run.
- Add MCP action `mempal_cowork_bus action=session_close`.
- Closing sets session status to `closed` and appends the existing runtime
  session-status event.
- Capture writes durable memory only when both `capture=true` and
  `execute=true` are supplied.

## Boundaries

### Allowed Changes
- specs/p101-cowork-session-close-capture.spec.md
- docs/plans/2026-05-26-p101-cowork-session-close-capture.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not capture raw tmux pane text.
- Do not automatically capture without an explicit capture flag.
- Do not promote knowledge or create cards.
- Do not alter legacy `mempal_cowork_push`.

## Acceptance Criteria

Scenario: CLI closes a session without writing memory
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_session_close_no_capture
    Targets: `cowork-session-close`.
  Given a runtime session exists
  When `mempal cowork-session-close --session-id review-1` runs
  Then `cowork-sessions --format json` reports status `closed`
  And no `palace.db` file is created

Scenario: CLI close capture execute writes evidence
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_session_close_capture_execute
    Targets: close + capture bridge.
  Given a runtime session exists
  When `cowork-session-close --capture --execute --format json` runs
  Then JSON includes `capture.writes=true`
  And the returned drawer is in wing `cowork-capture`

Scenario: MCP session_close mirrors CLI close and dry-run capture
  Test:
    Filter: test_mcp_cowork_bus_session_close
    Targets: `mempal_cowork_bus action=session_close`.
  Given a runtime session exists
  When MCP action `session_close` runs with `capture=true` and `execute=false`
  Then the response includes a closed session
  And the capture payload reports `writes=false`

## Out of Scope

- Automatic session expiration.
- Closing all sessions.
- Background capture.
