# P84 Multi-Agent Cowork Bus

Goal: upgrade cowork from tool-family pair routing (`claude` / `codex`) to a
project-scoped bus with concrete `agent_id` addressing and per-agent inboxes.

Spec: `specs/p84-multi-agent-cowork-bus.spec.md`

## Plan

- [x] Add P84 task contract and implementation plan.
- [x] Add failing CLI tests for register/list, unicast drain isolation,
      broadcast fanout, invalid addressing, and legacy compatibility.
- [x] Implement `src/cowork/bus.rs` with registry, agent id validation, and
      per-agent inbox helpers.
- [x] Wire CLI commands `cowork-register`, `cowork-send`,
      `cowork-broadcast`, `cowork-agent-drain`, and `cowork-agents`.
- [x] Update MIND-MODEL, AGENTS, and CLAUDE inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p84-multi-agent-cowork-bus.spec.md
agent-spec lint specs/p84-multi-agent-cowork-bus.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_register_and_agents_list
cargo test --test cowork_bus test_cli_cowork_send_drains_only_target_agent
cargo test --test cowork_bus test_cli_cowork_broadcast_fans_out_to_each_agent
cargo test --test cowork_bus test_cli_cowork_bus_rejects_invalid_addressing
cargo test --test cowork_bus test_legacy_cowork_status_still_lists_tool_inboxes
rg -n "p84-multi-agent-cowork-bus|P84 multi-agent cowork bus|cowork-register|cowork-send" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test --test cowork_bus
cargo test
cargo check --features rest
cargo clippy --features rest -- -D warnings
git diff --check
```
