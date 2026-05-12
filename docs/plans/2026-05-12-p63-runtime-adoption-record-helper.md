# P63 Runtime Adoption Record Helper

## Goal

Add a read-only helper that prepares exact runtime adoption `record` inputs
without appending an event.

## Scope

- Add `specs/p63-runtime-adoption-record-helper.spec.md`.
- Add CLI `mempal phase3 adoption prepare-record`.
- Add MCP `mempal_phase3 action=prepare_record`.
- Return `writes=false`, a CLI `record_command`, and an MCP `record_payload`.
- Update `MEMORY_PROTOCOL`, `MIND-MODEL-DESIGN.md`, `AGENTS.md`, and
  `CLAUDE.md`.
- Preserve read-only behavior and avoid schema/runtime default changes.

## Steps

- [x] Write failing CLI and MCP prepare-record tests.
- [x] Implement shared record helper output.
- [x] Wire CLI and MCP surfaces.
- [x] Update protocol, design, and repository inventories.
- [x] Run spec checks and Rust verification.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p63-runtime-adoption-record-helper.spec.md
agent-spec lint specs/p63-runtime-adoption-record-helper.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_json_is_read_only
cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_plain
cargo test --test phase3_runtime test_cli_phase3_adoption_prepare_record_rejects_invalid_track
cargo test mcp::server::tests::test_mcp_phase3_prepare_record_action_is_read_only --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
git diff --check
```
