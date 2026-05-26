# P90 Cowork Threads And Channels

Spec: `specs/p90-cowork-threads-channels.spec.md`

## Plan

- [x] Define P90 task contract and implementation plan.
- [x] Add thread/channel metadata to bus messages, events, and delivery status.
- [x] Add channel membership storage in the bus registry.
- [x] Add CLI `cowork-channel-set` and `cowork-channel-send`.
- [x] Add MCP `channel_set`, `channel_list`, and `channel_send` actions.
- [x] Add CLI/MCP tests for thread metadata and channel fanout.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p90-cowork-threads-channels.spec.md
agent-spec lint specs/p90-cowork-threads-channels.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_send_with_thread_metadata
cargo test --test cowork_bus test_cli_cowork_channel_send_fans_out
cargo test --test cowork_bus test_cli_cowork_channel_set_replaces_members
cargo test --test cowork_bus test_cli_cowork_channel_send_rejects_unknown_channel
cargo test --lib test_mcp_cowork_bus_channel_send
```

## Broader Regression

```bash
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test --test cowork_bus
cargo test --lib test_mcp_cowork_bus
cargo test --test cowork_inbox
cargo test
cargo check --features rest
cargo clippy --features rest -- -D warnings
git diff --check
```
