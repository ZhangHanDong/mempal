# P82 Opt-In Runtime Instrumentation Wrapper

Goal: implement the first explicit `opt_in_wrapper` runtime instrumentation path
without hooks, background capture, MCP command execution, or lifecycle authority.

Spec: `specs/p82-opt-in-runtime-instrumentation-wrapper.spec.md`

## Plan

- [x] Add P82 task contract and plan.
- [x] Add failing CLI tests for wrapper dry-run, execute, failure mapping,
      warning-quality block, and missing child command.
- [x] Implement CLI `mempal phase3 adoption wrap` in `src/main.rs`.
- [x] Reuse P72 capture mapping and P69 checked-record quality gate for all
      wrapper writes.
- [x] Update MEMORY_PROTOCOL, MIND-MODEL, AGENTS, and CLAUDE inventories.
- [x] Run spec validation and targeted tests.
- [x] Run full Rust verification before completion.

## Verification

```bash
agent-spec parse specs/p82-opt-in-runtime-instrumentation-wrapper.spec.md
agent-spec lint specs/p82-opt-in-runtime-instrumentation-wrapper.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_dry_run_executes_child_without_writing
cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_execute_writes_ready_event
cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_failure_maps_rejected_and_exits_nonzero
cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_blocks_warning_by_default
cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_rejects_missing_child_command
rg -n "p82-opt-in-runtime-instrumentation-wrapper|P82 opt-in runtime instrumentation wrapper|phase3 adoption wrap" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test
```
