# P60 MCP Phase-3 Runtime Surface

## Goal

Expose the P54-P59 Phase-3 runtime evidence baseline to MCP-connected agents
through one bounded `mempal_phase3` tool.

## Scope

- Add `specs/p60-mcp-phase3-runtime-surface.spec.md`.
- Add MCP request/response DTOs for Phase-3 runtime evidence.
- Add `mempal_phase3` MCP tool with actions:
  - `record`
  - `list`
  - `stats`
  - `gate`
  - `research_validate_plan`
- Update `MEMORY_PROTOCOL`, `docs/MIND-MODEL-DESIGN.md`, `AGENTS.md`, and
  `CLAUDE.md`.
- Do not change schema v9 or default runtime behavior.

## Steps

- [x] Write failing MCP tests for record/stats/gate, research validation, invalid
  action, and tool registry/protocol visibility.
- [x] Implement minimal `mempal_phase3` surface.
- [x] Update memory protocol.
- [x] Update design and repository inventories.
- [x] Run spec checks and Rust verification.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p60-mcp-phase3-runtime-surface.spec.md
agent-spec lint specs/p60-mcp-phase3-runtime-surface.spec.md --min-score 0.7
cargo test mcp::server::tests::test_mcp_phase3 --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
