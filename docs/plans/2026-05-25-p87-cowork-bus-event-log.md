# P87 Cowork Bus Event Log

Spec: `specs/p87-cowork-bus-event-log.spec.md`

## Plan

- [x] Define P87 task contract and implementation plan.
- [x] Add append-only `events.jsonl` storage helpers to `src/cowork/bus.rs`.
- [x] Record register, send/broadcast delivery, drain, and tmux failure events.
- [x] Expose CLI `cowork-events --cwd <path> [--limit N] [--format plain|json]`.
- [x] Expose MCP `mempal_cowork_bus action=events`.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p87-cowork-bus-event-log.spec.md
agent-spec lint specs/p87-cowork-bus-event-log.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_events_records_register_send_drain
cargo test --test cowork_bus test_cli_cowork_events_limit_returns_latest
cargo test --test cowork_bus test_cli_cowork_events_records_tmux_failure
cargo test --lib test_mcp_cowork_bus_events_lists_log
rg -n "p87-cowork-bus-event-log|P87 cowork bus event log|cowork-events|action=events" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
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
