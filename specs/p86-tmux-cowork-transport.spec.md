spec: task
name: "P86: tmux cowork transport"
inherits: project
tags: [cowork, multi-agent, tmux, transport, phase-4]
---

## Intent

P86 activates tmux as an explicit delivery transport for the P84/P85
multi-agent cowork bus. A registered `agent_id` can opt into `transport=tmux`
with a concrete tmux pane target, allowing near-real-time delivery to that pane
while preserving inbox as the safe default transport.

## Decisions

- `transport=inbox` remains the default and continues to append to per-agent
  inbox files.
- `transport=tmux` requires `tmux_target` at registration time.
- Sending to a `transport=tmux` target invokes the local `tmux` binary with
  direct `std::process::Command` arguments; no shell is used.
- The tmux payload is a plain text envelope containing source agent id, target
  agent id, and message content.
- tmux delivery does not write an inbox copy, avoiding duplicate delivery when
  the pane receives the message immediately.
- tmux command failure is a hard send failure and must not silently fall back
  to inbox.
- P86 reuses the existing P84 CLI and P85 MCP `send` / `broadcast` surfaces.

## Boundaries

### Allowed Changes
- specs/p86-tmux-cowork-transport.spec.md
- docs/plans/2026-05-25-p86-tmux-cowork-transport.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not make tmux the default transport.
- Do not execute tmux through a shell.
- Do not install hooks, daemons, watchers, or background processes.
- Do not write palace.db, drawers, cards, runtime adoption events, or schema
  state.
- Do not silently fall back to inbox if tmux delivery fails.
- Do not change legacy `mempal_cowork_push` behavior.

## Acceptance Criteria

Scenario: CLI sends to tmux transport through direct tmux command
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_send_to_tmux_transport_invokes_tmux
    Targets: CLI send path and tmux adapter.
  Given a fake `tmux` executable first in PATH
  And registered `claude-main` with `transport=inbox`
  And registered `codex-a` with `transport=tmux` and `tmux_target=mempal:0.1`
  When `mempal cowork-send --from claude-main --to codex-a` sends a message
  Then the fake tmux log records `send-keys`, `-t`, `mempal:0.1`, and the message envelope
  And `mempal cowork-agent-drain --agent-id codex-a` returns no inbox message

Scenario: CLI rejects tmux transport without target
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_register_tmux_requires_target
    Targets: registration validation.
  Given a project cwd
  When registering `codex-a` with `transport=tmux` and no `tmux_target`
  Then the command fails
  And stderr contains `tmux_target is required`

Scenario: CLI tmux failure is not silently converted to inbox delivery
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_tmux_failure_does_not_write_inbox
    Targets: failure boundary.
  Given a fake `tmux` executable that exits non-zero
  And registered `codex-a` with `transport=tmux`
  When sending a message to `codex-a`
  Then the send command fails
  And draining `codex-a` returns no message

Scenario: MCP send uses tmux transport through shared bus core
  Test:
    Filter: test_mcp_cowork_bus_send_to_tmux_transport_invokes_tmux
    Targets: src/mcp/server.rs MCP send path and shared bus core.
  Given fake tmux in PATH
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  And registered `codex-a` with `transport=tmux` through `mempal_cowork_bus`
  When `mempal_cowork_bus action=send` targets `codex-a`
  Then the response reports delivery transport `tmux`
  And fake tmux receives the message envelope

## Out of Scope

- tmux pane discovery.
- tmux capture-pane based peek.
- Retrying failed tmux sends.
- Cross-machine tmux or SSH transport.
- Shell-specific quoting behavior.
