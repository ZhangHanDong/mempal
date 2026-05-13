# P79 Rollback Executor Policy

Spec: `specs/p79-rollback-executor-policy.spec.md`

## Checklist

- [x] Add P79 task contract and plan.
- [x] Add core rollback-control report logic for `card-context`.
- [x] Add CLI `mempal phase3 rollback-control card-context`.
- [x] Add MCP `mempal_phase3 action=rollback_control`.
- [x] Update protocol, AGENTS/CLAUDE inventory, and MIND-MODEL-DESIGN.
- [x] Verify spec parse/lint, targeted tests, fmt/check/clippy/test, and diff check.

## Verification

```bash
agent-spec parse specs/p79-rollback-executor-policy.spec.md
agent-spec lint specs/p79-rollback-executor-policy.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_rollback_control_check_is_read_only
cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_disables_default
cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_without_signal_is_noop
cargo test --test phase3_runtime test_cli_phase3_rollback_control_execute_already_disabled_is_noop
cargo test --test phase3_runtime test_cli_phase3_rollback_control_rejects_unknown_candidate_without_config_write
cargo test mcp::server::tests::test_mcp_phase3_rollback_control_check_is_read_only --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
cargo test --features rest
git diff --check
```
