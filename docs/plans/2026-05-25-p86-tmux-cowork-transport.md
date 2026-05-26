# P86 Tmux Cowork Transport

Goal: make `transport=tmux` an active opt-in delivery path for concrete
`agent_id` bus targets while keeping `inbox` as the safe default.

Spec: `specs/p86-tmux-cowork-transport.spec.md`

## Plan

- [x] Add P86 task contract and implementation plan.
- [x] Add failing CLI and MCP tests using fake `tmux` binaries.
- [x] Extend bus delivery reports for transport-aware output.
- [x] Implement direct `tmux send-keys` delivery with no shell and hard failure
      on non-zero exit.
- [x] Update MIND-MODEL, AGENTS, and CLAUDE inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p86-tmux-cowork-transport.spec.md
agent-spec lint specs/p86-tmux-cowork-transport.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_send_to_tmux_transport_invokes_tmux
cargo test --test cowork_bus test_cli_cowork_register_tmux_requires_target
cargo test --test cowork_bus test_cli_cowork_tmux_failure_does_not_write_inbox
cargo test --lib test_mcp_cowork_bus_send_to_tmux_transport_invokes_tmux
rg -n "p86-tmux-cowork-transport|P86 tmux cowork transport|transport=tmux|tmux_target" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
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
