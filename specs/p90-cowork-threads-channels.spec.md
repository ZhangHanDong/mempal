spec: task
name: "P90: cowork threads and channels"
inherits: project
tags: [cowork, multi-agent, threads, channels, phase-4]
---

## Intent

P90 adds thread and channel routing metadata to the multi-agent cowork bus.
Agents should be able to keep work streams separated by `thread_id`, and
operators should be able to send one message to a named channel without
retyping every agent id.

## Decisions

- Add optional `thread_id` and `channel` metadata to bus messages, delivery
  events, and delivery status replay.
- Add channel membership to the existing file-backed bus registry.
- `cowork-channel-set` replaces the membership list for one channel.
- `cowork-channel-send` fans out to the current channel members using the same
  delivery core as broadcast.
- Channel and thread identifiers use the same safe token rules as `agent_id`.
- MCP reuses `mempal_cowork_bus` actions `channel_set`, `channel_list`, and
  `channel_send`.
- Existing `cowork-send` and `cowork-broadcast` remain valid without thread or
  channel metadata.

## Boundaries

### Allowed Changes
- specs/p90-cowork-threads-channels.spec.md
- docs/plans/2026-05-25-p90-cowork-threads-channels.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/cowork/bus.rs
- src/cowork/inbox.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/cowork_bus.rs
- tests/cowork_inbox.rs

### Forbidden
- Do not add SQLite schema, migrations, or `palace.db` writes.
- Do not make channel membership implicit from tool family names.
- Do not send to unknown channel members.
- Do not change legacy `mempal_cowork_push` or `mempal_peek_partner`
  behavior.
- Do not implement threaded search or persistent memory ingestion.

## Acceptance Criteria

Scenario: CLI send preserves thread metadata in drain output and events
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_send_with_thread_metadata
    Targets: `cowork-send --thread-id` and bus inbox formatting.
  Given registered `claude-main` and `codex-a`
  When `cowork-send --thread-id p90-review` sends a message
  Then draining `codex-a` includes `thread=p90-review`
  And `cowork-events` includes `thread_id=p90-review`

Scenario: CLI channel send fans out to channel members
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_channel_send_fans_out
    Targets: `cowork-channel-set` and `cowork-channel-send`.
  Given channel `review` contains `codex-a` and `codex-b`
  When `cowork-channel-send --channel review` sends a message
  Then both `codex-a` and `codex-b` drain that message
  And delivery status includes `channel=review`

Scenario: CLI channel set replaces existing membership
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_channel_set_replaces_members
    Targets: `cowork-channel-set` replacement behavior.
  Given channel `review` already contains `codex-a` and `codex-b`
  When `cowork-channel-set --channel review --agent codex-b` runs
  Then `cowork-channel-send --channel review` sends only to `codex-b`
  And `codex-a` drains no message

Scenario: CLI channel send rejects unknown channel
  Test:
    Filter: cargo test --test cowork_bus test_cli_cowork_channel_send_rejects_unknown_channel
    Targets: channel validation.
  Given no channel named `missing`
  When `cowork-channel-send --channel missing` runs
  Then the command fails
  And stderr contains `unknown channel`

Scenario: MCP channel actions use shared routing
  Test:
    Filter: test_mcp_cowork_bus_channel_send
    Targets: `src/mcp/server.rs` `mempal_cowork_bus action=channel_set|channel_send`.
  Given channel `review` is set through MCP
  And the MCP server handler in `src/mcp/server.rs` is the exercised entry point
  When `action=channel_send` sends a message
  Then delivery reports target both channel members
  And no `palace.db` file is created

## Out of Scope

- Thread-specific search or context assembly.
- Channel membership discovery from tmux sessions.
- Role-based permissions.
- Cross-project channels.
- Automatic channel cleanup.
