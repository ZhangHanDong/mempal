spec: task
name: "P87: cowork bus event log"
inherits: project
tags: [cowork, multi-agent, event-log, replay, phase-4]
---

## Intent

P87 adds an append-only operational event log to the P84-P86 multi-agent
cowork bus. The bus can already route messages between concrete agent
instances; this task makes those actions inspectable and replayable as an
audit trail without writing `palace.db`.

## Decisions

- Store bus events as JSON Lines at
  `~/.mempal/cowork-bus/<encoded_project_identity>/events.jsonl`.
- Record successful register, send, broadcast delivery, and drain actions.
- Record tmux delivery failure events before returning the hard send failure.
- Event replay means listing the recorded operational events; it does not
  redeliver messages or mutate inbox state.
- Expose replay through CLI command `cowork-events` and MCP action
  `mempal_cowork_bus action=events`.
- Keep message content as a bounded `message_preview`, not a second full
  durable message store.

## Boundaries

### Allowed Changes
- specs/p87-cowork-bus-event-log.spec.md
- docs/plans/2026-05-25-p87-cowork-bus-event-log.md
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
- Do not write `palace.db`, drawers, cards, runtime adoption events, or schema
  state.
- Do not make event replay redeliver inbox or tmux messages.
- Do not store full message bodies in event records.
- Do not change legacy `mempal_cowork_push` or `mempal_peek_partner`
  behavior.
- Do not introduce a daemon, watcher, hook, or background event collector.

## Acceptance Criteria

Scenario: CLI replay records register, send, and drain events
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_events_records_register_send_drain
    Targets: `cowork-register`, `cowork-send`, `cowork-agent-drain`, `cowork-events`, and events.jsonl file output.
  Given a project with registered `claude-main` and `codex-a`
  When `claude-main` sends an inbox message to `codex-a`
  And `codex-a` drains its inbox
  Then `mempal cowork-events --cwd <repo>` prints register, send, and drain events
  And the output includes event ids, status values, actor agent ids, and target agent ids
  And the event file exists under `cowork-bus/<project>/events.jsonl`

Scenario: CLI JSON replay is machine-readable
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_events_json_is_machine_readable
    Targets: `cowork-events --format json` stdout behavior.
  Given recorded bus events
  When `mempal cowork-events --cwd <repo> --format json` runs
  Then stdout parses as a JSON array of event records
  And stderr is empty

Scenario: JSONL file output is append-only
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_events_file_output_is_append_only
    Targets: `events.jsonl` file output.
  Given an existing `events.jsonl` file with register events
  When a send event is recorded
  Then the file output gains one JSON line
  And the existing event lines remain unchanged

Scenario: CLI replay limit returns the latest events only
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_events_limit_returns_latest
    Targets: `cowork-events --limit`.
  Given more than two recorded bus events
  When `mempal cowork-events --cwd <repo> --limit 2` runs
  Then exactly two event lines are printed
  And they correspond to the latest recorded events in append order

Scenario: CLI logs tmux delivery failure without inbox fallback
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_events_records_tmux_failure
    Targets: tmux failure path and event log append.
  Given a fake `tmux` executable that exits non-zero
  And registered `codex-a` with `transport=tmux`
  When sending a message to `codex-a`
  Then the send command fails
  And `cowork-events` includes a failed tmux delivery event
  And draining `codex-a` returns no inbox message

Scenario: MCP events action replays the shared bus event log
  Test:
    Filter: test_mcp_cowork_bus_events_lists_log
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=events`.
  Given bus events created through `mempal_cowork_bus`
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  When `mempal_cowork_bus action=events` is called with a limit
  Then the response includes recorded event DTOs
  And it does not drain inbox messages or write database state

## Out of Scope

- Delivery acknowledgements or pending/acked state machines.
- Agent heartbeats or online/stale presence.
- Thread, channel, or group routing.
- tmux `capture-pane` live peek.
- Durable memory ingestion of cowork events.
