spec: task
name: "P91: tmux live peek"
inherits: project
tags: [cowork, multi-agent, tmux, peek, phase-4]
---

## Intent

P91 adds explicit read-only tmux pane peek for agents registered with
`transport=tmux`. This complements P86 tmux send by letting an operator or
agent inspect the live pane state without changing inboxes, events, or memory.

## Decisions

- Add CLI `cowork-tmux-peek --agent-id <id> --cwd <repo>`.
- Add MCP action `mempal_cowork_bus action=tmux_peek`.
- Only agents registered with `transport=tmux` and `tmux_target` can be peeked.
- Use direct `std::process::Command` invocation of `tmux capture-pane`; do not
  execute through a shell.
- Default capture size is 80 lines; accepted line range is 1..=500.
- tmux capture failure is a hard error and does not fall back to inbox or
  legacy live session peek.
- The operation is read-only: no `events.jsonl`, inbox, registry, or
  `palace.db` writes.

## Boundaries

### Allowed Changes
- specs/p91-tmux-live-peek.spec.md
- docs/plans/2026-05-25-p91-tmux-live-peek.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not execute tmux through a shell.
- Do not mutate bus events, inbox messages, channel registry, or delivery
  status during peek.
- Do not fall back to `mempal_peek_partner`.
- Do not implement tmux pane discovery.
- Do not change legacy `mempal_peek_partner` behavior.

## Acceptance Criteria

Scenario: CLI tmux peek captures registered pane
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_tmux_peek_captures_pane
    Targets: `src/main.rs` `cowork-tmux-peek` and direct tmux capture adapter.
  Given fake `tmux` first in PATH
  And the CLI handler in `src/main.rs` is the exercised entry point
  And registered `codex-a` with `transport=tmux` and `tmux_target=mempal:0.1`
  When `cowork-tmux-peek --agent-id codex-a --lines 20` runs
  Then stdout contains the fake pane output
  And stderr is empty
  And the fake tmux log records `capture-pane`, `-t`, `mempal:0.1`, `-p`, `-S`, and `-20`

Scenario: CLI tmux peek rejects non-tmux agent
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_tmux_peek_rejects_inbox_agent
    Targets: transport validation.
  Given registered `codex-a` with default `transport=inbox`
  When `cowork-tmux-peek --agent-id codex-a` runs
  Then the command fails
  And stderr contains `not registered with transport=tmux`

Scenario: CLI tmux peek has no bus side effects
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_tmux_peek_has_no_bus_side_effects
    Targets: read-only boundary and events.jsonl file output.
  Given a registered tmux agent and an existing event log
  When `cowork-tmux-peek` runs
  Then `events.jsonl` is unchanged
  And no `palace.db` file is created

Scenario: CLI tmux peek does not write file output
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_tmux_peek_does_not_write_file_output
    Targets: absence of file-output side effects.
  Given a registered tmux agent
  When `cowork-tmux-peek` runs
  Then no new file-output artifact is created under the project bus directory

Scenario: MCP tmux peek uses the same capture adapter
  Test:
    Filter: test_mcp_cowork_bus_tmux_peek
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=tmux_peek`.
  Given fake tmux in PATH
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  When `action=tmux_peek` targets a registered tmux agent
  Then the response includes captured pane text
  And no `palace.db` file is created

## Out of Scope

- tmux pane discovery.
- Streaming pane follow mode.
- Cross-machine or SSH tmux.
- OCR or semantic parsing of terminal output.
- Persisting captured pane text as memory.
