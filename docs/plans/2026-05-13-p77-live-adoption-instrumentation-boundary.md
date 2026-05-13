# P77 Live Adoption Instrumentation Boundary

Goal: expose a deterministic read-only Phase-3 policy surface that defines which
live adoption instrumentation modes are allowed before any hooks or tool wrappers
are implemented.

Spec: `specs/p77-live-adoption-instrumentation-boundary.spec.md`

## Steps

- [x] Add P77 task contract and plan.
- [x] Add failing CLI tests for `mempal phase3 adoption instrumentation-policy`.
- [x] Add failing MCP test for `mempal_phase3 action=instrumentation_policy`.
- [x] Implement pure instrumentation policy DTOs in `src/core/phase3.rs`.
- [x] Wire CLI and MCP read-only surfaces.
- [x] Update protocol text, AGENTS/CLAUDE inventories, and MIND-MODEL summary.
- [x] Verify spec parse/lint, targeted tests, fmt/check/clippy/test, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p77-live-adoption-instrumentation-boundary.spec.md
agent-spec lint specs/p77-live-adoption-instrumentation-boundary.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_instrumentation_policy_json_is_read_only
cargo test --test phase3_runtime test_cli_phase3_adoption_instrumentation_policy_rejects_invalid_format
cargo test mcp::server::tests::test_mcp_phase3_instrumentation_policy_action_is_read_only --lib
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_phase3_runtime_surface --lib
rg -n "p77-live-adoption-instrumentation-boundary|P77 live adoption instrumentation boundary" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
git diff --check
```
