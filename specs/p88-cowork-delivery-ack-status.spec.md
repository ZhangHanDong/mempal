spec: task
name: "P88: cowork delivery ack status"
inherits: project
tags: [cowork, multi-agent, ack, status, phase-4]
---

## Intent

P88 adds delivery-level acknowledgement and status inspection on top of the
P87 cowork bus event log. Operators and agents should be able to see whether a
bus delivery is pending, drained, acked, or failed without introducing a
database table or background worker.

## Decisions

- Use the P87 delivery event id as the `message_id` exposed by send/broadcast.
- Derive delivery status by replaying `events.jsonl`; do not add a sidecar
  mutable status file.
- `pending` means delivered but not yet drained or acked.
- `drained` means a later drain event consumed the target inbox message.
- `acked` means the target agent explicitly appended an ack event for that
  `message_id`.
- `failed` means the original delivery event recorded a hard delivery failure.
- Expose status through CLI `cowork-deliveries` and MCP action `deliveries`.
- Expose explicit acknowledgement through CLI `cowork-ack` and MCP action
  `ack`.

## Boundaries

### Allowed Changes
- specs/p88-cowork-delivery-ack-status.spec.md
- docs/plans/2026-05-25-p88-cowork-delivery-ack-status.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs

### Forbidden
- Do not add SQLite schema, tables, migrations, or `palace.db` writes.
- Do not add daemons, watchers, hooks, or automatic background ack capture.
- Do not make ack mutate inbox message files.
- Do not acknowledge failed deliveries.
- Do not change legacy `mempal_cowork_push` or `mempal_peek_partner`
  behavior.

## Acceptance Criteria

Scenario: CLI send exposes message id and pending status
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_delivery_status_pending
    Targets: `cowork-send` and `cowork-deliveries`.
  Given `claude-main` sends an inbox delivery to `codex-a`
  When `mempal cowork-deliveries --cwd <repo>` runs
  Then the send output includes `message_id=evt-...`
  And the delivery status output includes the same message id
  And the status is `pending`

Scenario: CLI drain changes pending delivery to drained
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_delivery_status_drained
    Targets: `cowork-agent-drain` and event-log status replay.
  Given a pending inbox delivery for `codex-a`
  When `codex-a` drains its inbox
  Then `cowork-deliveries --agent-id codex-a` reports that delivery as `drained`

Scenario: CLI ack marks a delivery acked without touching inbox
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_ack_marks_delivery_acked
    Targets: `cowork-ack` and `cowork-deliveries`.
  Given a pending inbox delivery for `codex-a`
  When `mempal cowork-ack --agent-id codex-a --message-id <id>` runs
  Then `cowork-deliveries --agent-id codex-a` reports status `acked`
  And draining `codex-a` still returns the original message

Scenario: CLI failed tmux delivery reports failed and cannot be acked
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_failed_delivery_cannot_be_acked
    Targets: tmux failure status and ack rejection.
  Given a failed tmux delivery event for `codex-a`
  When `cowork-deliveries --agent-id codex-a` runs
  Then the status is `failed`
  And `cowork-ack --agent-id codex-a --message-id <failed_id>` fails

Scenario: MCP deliveries and ack actions use the same event replay
  Test:
    Filter: test_mcp_cowork_bus_deliveries_and_ack
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=deliveries|ack`.
  Given a delivery created through `mempal_cowork_bus action=send`
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  When `action=ack` acknowledges its message id
  Then `action=deliveries` returns status `acked`
  And no `palace.db` file is created

## Out of Scope

- Automatic ack from hook execution.
- Per-message retry or redelivery.
- Cross-process locking around status replay.
- Thread or channel routing.
- Presence or heartbeat status.
