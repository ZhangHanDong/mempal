# P88 Cowork Delivery Ack Status

Spec: `specs/p88-cowork-delivery-ack-status.spec.md`

## Plan

- [x] Define P88 task contract and implementation plan.
- [x] Add delivery status replay over P87 `events.jsonl`.
- [x] Expose delivery event id as `message_id` from CLI/MCP send reports.
- [x] Add CLI `cowork-deliveries` and `cowork-ack`.
- [x] Add MCP actions `deliveries` and `ack`.
- [x] Add CLI/MCP tests for pending, drained, acked, and failed states.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p88-cowork-delivery-ack-status.spec.md
agent-spec lint specs/p88-cowork-delivery-ack-status.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_delivery_status_pending
cargo test --test cowork_bus test_cli_cowork_delivery_status_drained
cargo test --test cowork_bus test_cli_cowork_ack_marks_delivery_acked
cargo test --test cowork_bus test_cli_cowork_failed_delivery_cannot_be_acked
cargo test --lib test_mcp_cowork_bus_deliveries_and_ack
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
