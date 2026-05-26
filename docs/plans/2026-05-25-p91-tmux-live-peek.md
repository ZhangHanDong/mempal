# P91 Tmux Live Peek

Spec: `specs/p91-tmux-live-peek.spec.md`

## Plan

- [x] Define P91 task contract and implementation plan.
- [x] Add read-only tmux capture adapter to bus core.
- [x] Add CLI `cowork-tmux-peek`.
- [x] Add MCP `mempal_cowork_bus action=tmux_peek`.
- [x] Add CLI/MCP tests for success, rejection, and read-only side effects.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p91-tmux-live-peek.spec.md
agent-spec lint specs/p91-tmux-live-peek.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_tmux_peek_captures_pane
cargo test --test cowork_bus test_cli_cowork_tmux_peek_rejects_inbox_agent
cargo test --test cowork_bus test_cli_cowork_tmux_peek_has_no_bus_side_effects
cargo test --test cowork_bus test_cli_cowork_tmux_peek_does_not_write_file_output
cargo test --lib test_mcp_cowork_bus_tmux_peek
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
