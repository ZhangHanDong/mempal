# P68 MCP Research Ingest Plan

Goal: expose P67 research evidence ingest planning through `mempal_phase3` as a
read-only MCP action, while keeping all writes limited to the existing CLI
`--execute` path.

Spec: `specs/p68-mcp-research-ingest-plan.spec.md`

## Steps

- [x] Add P68 task contract and plan.
- [x] Add RED MCP tests for valid dry-run, invalid report, action list, and protocol registry.
- [x] Move P67 pure planning helpers into shared `src/core/phase3.rs`.
- [x] Wire CLI and MCP to the shared planner without changing P67 CLI behavior.
- [x] Update MIND-MODEL, AGENTS, and CLAUDE docs.
- [x] Verify targeted tests, full local checks, spec parse/lint, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p68-mcp-research-ingest-plan.spec.md
agent-spec lint specs/p68-mcp-research-ingest-plan.spec.md --min-score 0.7
cargo test mcp::server::tests::test_mcp_phase3_research_ingest_plan_is_read_only --lib
cargo test mcp::server::tests::test_mcp_phase3_research_ingest_plan_invalid_report_no_write --lib
cargo test mcp::server::tests::test_mcp_phase3_rejects_invalid_action_without_mutation --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_dry_run_json_no_write
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rest -- -D warnings
cargo test
git diff --check
```
