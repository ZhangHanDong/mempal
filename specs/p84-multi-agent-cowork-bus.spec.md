spec: task
name: "P84: multi-agent cowork bus"
inherits: project
tags: [cowork, multi-agent, cli, inbox, phase-4]
---

## Intent

P84 upgrades cowork from a Claude/Codex pair protocol into a concrete
multi-agent bus for several agent instances working in the same project. The
bus introduces stable `agent_id` addressing, a project-scoped agent registry,
and per-agent inbox files so that one Claude instance and multiple Codex
instances can exchange messages without racing on the same `codex` inbox.

## Decisions

- Add a project-scoped multi-agent registry keyed by `agent_id`.
- Store bus state under `~/.mempal/cowork-bus/<encoded_project_identity>/`.
- Keep legacy `cowork-drain --target claude|codex` and `mempal_cowork_push`
  behavior unchanged for backward compatibility.
- Add CLI commands `cowork-register`, `cowork-send`, `cowork-broadcast`,
  `cowork-agent-drain`, and `cowork-agents`.
- `cowork-send` and `cowork-broadcast` address concrete `agent_id` values, not
  tool families.
- Each target `agent_id` has its own inbox file; broadcast writes one message to
  each target inbox.
- The bus is file-backed and ephemeral like P8 cowork inbox; it must not write
  palace.db, drawers, cards, runtime adoption events, or schema state.
- P84 records optional `transport` metadata but only `inbox` delivery is active.
  tmux delivery is reserved for a later P and must not be silently attempted.
- `agent_id` accepts ASCII letters, digits, `_`, `-`, and `.`, with length 1-64.

## Boundaries

### Allowed Changes
- specs/p84-multi-agent-cowork-bus.spec.md
- docs/plans/2026-05-25-p84-multi-agent-cowork-bus.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/**
- src/main.rs
- tests/cowork_bus.rs

### Forbidden
- Do not change palace.db schema or `CURRENT_SCHEMA_VERSION`.
- Do not write drawers, triples, cards, runtime adoption events, or audit rows.
- Do not remove or change legacy P8 `cowork-drain`, `cowork-status`, or
  `cowork-install-hooks` behavior.
- Do not make `mempal_cowork_push` require `agent_id` in P84.
- Do not implement tmux send/capture in P84.
- Do not introduce new runtime dependencies.

## Acceptance Criteria

Scenario: CLI registers concrete agent instances in one project
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_register_and_agents_list
    Targets: CLI registry behavior and project-scoped state.
  Given a temporary HOME and one project cwd
  When registering `claude-main`, `codex-a`, and `codex-b`
  Then `mempal cowork-agents --cwd <project>` lists all three agent ids
  And each record preserves its tool name and transport
  And the registry file exists under `~/.mempal/cowork-bus/<encoded_project_identity>/agents.json`
  And no palace.db file is created

Scenario: CLI send targets one Codex instance without leaking to another
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_send_drains_only_target_agent
    Targets: per-agent inbox routing.
  Given registered agents `claude-main`, `codex-a`, and `codex-b` in the same project
  When `claude-main` sends a message to `codex-a`
  Then `mempal cowork-agent-drain --agent-id codex-a` prints the message
  And `mempal cowork-agent-drain --agent-id codex-b` prints nothing
  And a second drain of `codex-a` prints nothing

Scenario: CLI broadcast fans out independent inbox copies
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_broadcast_fans_out_to_each_agent
    Targets: broadcast fanout semantics.
  Given registered agents `claude-main`, `codex-a`, and `codex-b`
  When `claude-main` broadcasts one message to `codex-a` and `codex-b`
  Then draining `codex-a` returns one copy
  And draining `codex-b` returns one copy
  And neither drain consumes the other agent's inbox

Scenario: CLI rejects invalid addressing without side effects
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_bus_rejects_invalid_addressing
    Targets: validation and failure behavior.
  Given a registered `codex-a`
  When registering agent id `bad/id`
  Then the command fails with an invalid agent id error
  When `codex-a` sends to itself
  Then the command fails with a self-send error
  When sending to missing `codex-missing`
  Then the command fails with an unknown target error

Scenario: Legacy cowork pair behavior remains available
  Test:
    Filter: cargo test --test cowork_bus test_legacy_cowork_status_still_lists_tool_inboxes
    Targets: P8 compatibility boundary.
  Given a project cwd
  When running `mempal cowork-status --cwd <project>`
  Then stdout still contains `claude inbox`
  And stdout still contains `codex inbox`
  And it does not require any multi-agent registry file

## Out of Scope

- MCP multi-agent bus surface.
- tmux send/capture transport.
- Hook auto-installation for individual `agent_id` values.
- Cross-machine transport.
- Durable message history after drain.
- Message threading, acknowledgement, or retry protocol.
