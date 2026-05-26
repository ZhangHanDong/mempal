# P89 Cowork Agent Presence

Spec: `specs/p89-cowork-agent-presence.spec.md`

## Plan

- [x] Define P89 task contract and implementation plan.
- [x] Add `last_seen_at` and read-time presence derivation to bus registry.
- [x] Add CLI `cowork-heartbeat` and extend `cowork-agents` output.
- [x] Add MCP `heartbeat` action and list presence DTO fields.
- [x] Add deterministic CLI/MCP tests with explicit timestamps.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p89-cowork-agent-presence.spec.md
agent-spec lint specs/p89-cowork-agent-presence.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_heartbeat_updates_presence
cargo test --test cowork_bus test_cli_cowork_agents_marks_stale_presence
cargo test --test cowork_bus test_cli_cowork_heartbeat_rejects_unknown_agent
cargo test --lib test_mcp_cowork_bus_heartbeat_and_presence
```

## Broader Regression

```bash
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test --test cowork_bus
cargo test --lib test_mcp_cowork_bus
cargo test
cargo check --features rest
cargo clippy --features rest -- -D warnings
git diff --check
```
