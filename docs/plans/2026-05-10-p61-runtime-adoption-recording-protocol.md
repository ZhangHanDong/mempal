# P61 Runtime Adoption Recording Protocol

## Goal

Define deterministic guidance for when agents should record Phase-3 runtime
adoption events through `mempal_phase3`.

## Scope

- Add `specs/p61-runtime-adoption-recording-protocol.spec.md`.
- Extend `mempal_phase3` with read-only `action=guidance`.
- Add guidance DTOs for required fields, optional fields, signal semantics, and
  track semantics.
- Update `MEMORY_PROTOCOL`, `docs/MIND-MODEL-DESIGN.md`, `AGENTS.md`, and
  `CLAUDE.md`.
- Do not add automatic event recording or runtime hooks.

## Steps

- [x] Write failing MCP tests for guidance response and protocol visibility.
- [x] Implement read-only guidance action.
- [x] Update memory protocol with recording semantics.
- [x] Update design and repository inventories.
- [x] Run spec checks and Rust verification.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p61-runtime-adoption-recording-protocol.spec.md
agent-spec lint specs/p61-runtime-adoption-recording-protocol.spec.md --min-score 0.7
cargo test mcp::server::tests::test_mcp_phase3_guidance_action_is_read_only --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
