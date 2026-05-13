# P72 Runtime Adoption Capture Helper

Goal: add an explicit capture helper that turns low-friction
`surface/outcome` runtime observations into existing checked runtime adoption
records.

Spec: `specs/p72-runtime-adoption-capture-helper.spec.md`

## Steps

- [x] Add P72 task contract and plan.
- [x] Add failing CLI capture tests for dry-run, execute, warning block, and invalid surface.
- [x] Add failing MCP capture test for dry-run and execute.
- [x] Implement pure capture mapping and checked-write reuse in `src/core/phase3.rs`.
- [x] Wire CLI `mempal phase3 adoption capture`.
- [x] Wire MCP `mempal_phase3 action=capture`.
- [x] Update protocol/tool descriptions and inventories.
- [x] Verify spec parse/lint, targeted tests, formatting, clippy, full tests, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p72-runtime-adoption-capture-helper.spec.md
agent-spec lint specs/p72-runtime-adoption-capture-helper.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_capture_card_context_dry_run
cargo test --test phase3_runtime test_cli_phase3_adoption_capture_execute_writes_ready_event
cargo test --test phase3_runtime test_cli_phase3_adoption_capture_blocks_warning_by_default
cargo test --test phase3_runtime test_cli_phase3_adoption_capture_rejects_unknown_surface
cargo test mcp::server::tests::test_mcp_phase3_capture_action_dry_run_and_execute --lib
rg -n "p72-runtime-adoption-capture-helper|P72 runtime adoption capture helper" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
git diff --check
```
