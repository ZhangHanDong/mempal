spec: task
name: "P95: cowork handoff summary"
inherits: project
tags: [cowork, multi-agent, handoff, mcp, phase-4]
---

## Intent

P95 adds a deterministic handoff summary for multi-agent cowork state. A new or
returning agent should be able to inspect active sessions, recent events,
pending deliveries, stale agents, and thread/channel context without reading
raw JSONL files or relying on an LLM-generated summary.

## Decisions

- Add CLI `mempal cowork-handoff --cwd <repo>`.
- Add MCP action `mempal_cowork_bus action=handoff`.
- Support optional filters `--thread-id`, `--channel`, `--session-id`, and
  `--limit`.
- Output formats are `plain` and `json`.
- The summary is read-only and does not write events, inboxes, sessions, or
  `palace.db`.

## Boundaries

### Allowed Changes
- specs/p95-cowork-handoff-summary.spec.md
- docs/plans/2026-05-26-p95-cowork-handoff-summary.md
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
- Do not call an LLM.
- Do not persist the summary as memory.
- Do not drain inboxes or ack deliveries.

## Acceptance Criteria

Scenario: CLI handoff summarizes sessions and pending deliveries
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_handoff_plain
    Targets: `cowork-handoff` plain output.
  Given an active session and a pending delivery
  When `mempal cowork-handoff --cwd <repo>` runs
  Then stdout contains `Active sessions`
  And stdout contains the session id
  And stdout contains `Pending deliveries`

Scenario: CLI handoff filters by thread id
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_handoff_filters_thread
    Targets: thread filter behavior.
  Given deliveries for `thread-a` and `thread-b`
  When `cowork-handoff --thread-id thread-a --format json` runs
  Then JSON includes `thread-a`
  And JSON does not include `thread-b`

Scenario: CLI handoff rejects invalid format without side effects
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_handoff_rejects_invalid_format
    Targets: format validation and read-only boundary.
  Given an existing event log
  When `cowork-handoff --format yaml` runs
  Then the command fails
  And `events.jsonl` is unchanged

Scenario: MCP handoff returns the same summary shape
  Test:
    Filter: test_mcp_cowork_bus_handoff
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=handoff`.
  Given an active session and pending delivery
  When MCP action `handoff` runs
  Then the response includes a handoff payload
  And no `palace.db` file is created

## Out of Scope

- Natural-language LLM summarization.
- Automatic memory capture.
- Cross-project handoffs.
- Modifying delivery status.
