# P85 MCP Multi-Agent Cowork Bus

Goal: expose the P84 concrete `agent_id` cowork bus through one MCP tool so
agents can use multi-instance routing without shell-only coordination.

Spec: `specs/p85-mcp-multi-agent-cowork-bus.spec.md`

## Plan

- [x] Add P85 task contract and implementation plan.
- [x] Add failing MCP handler tests for register/list, send/drain isolation,
      broadcast fanout, invalid addressing, and protocol/tool registry.
- [x] Add `mempal_cowork_bus` DTOs in `src/mcp/tools.rs`.
- [x] Implement action-based MCP handler in `src/mcp/server.rs`.
- [x] Update MEMORY_PROTOCOL, MIND-MODEL, AGENTS, and CLAUDE inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p85-mcp-multi-agent-cowork-bus.spec.md
agent-spec lint specs/p85-mcp-multi-agent-cowork-bus.spec.md --min-score 0.7
cargo test --lib test_mcp_cowork_bus_register_and_list
cargo test --lib test_mcp_cowork_bus_send_drains_only_target
cargo test --lib test_mcp_cowork_bus_broadcast_fans_out
cargo test --lib test_mcp_cowork_bus_rejects_invalid_action_and_addressing
cargo test --lib test_mcp_tool_registry_and_protocol_include_cowork_bus
rg -n "p85-mcp-multi-agent-cowork-bus|P85 MCP multi-agent cowork bus|mempal_cowork_bus|agent_id" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test --lib test_mcp_cowork_bus
cargo test
cargo check --features rest
cargo clippy --features rest -- -D warnings
git diff --check
```
