# P69 Runtime Adoption Checked Record

Goal: add a quality-gated runtime adoption write path so agents can append
Phase-3 evidence only after the P64 policy has been evaluated.

Spec: `specs/p69-runtime-adoption-checked-record.spec.md`

## Steps

- [x] Add P69 task contract and plan.
- [x] Add RED CLI and MCP tests for ready, warning-blocked, warning-allowed, and invalid-blocked behavior.
- [x] Add checked-record response types and core decision helper.
- [x] Wire CLI `mempal phase3 adoption record-checked`.
- [x] Wire MCP `mempal_phase3 action=record_checked`.
- [x] Update MIND-MODEL, AGENTS, and CLAUDE docs.
- [x] Verify targeted tests, full local checks, spec parse/lint, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p69-runtime-adoption-checked-record.spec.md
agent-spec lint specs/p69-runtime-adoption-checked-record.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_writes_ready_event
cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_blocks_warning_by_default
cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_allow_warnings_writes_warning_event
cargo test --test phase3_runtime test_cli_phase3_adoption_record_checked_blocks_invalid_even_with_allow_warnings
cargo test mcp::server::tests::test_mcp_phase3_record_checked_quality_gated --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rest -- -D warnings
cargo test
git diff --check
```
