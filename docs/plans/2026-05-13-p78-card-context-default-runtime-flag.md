# P78 Card Context Default Runtime Flag

Goal: add an explicit, reversible runtime config flag for card-aware context
defaults, gated by P74 proposal readiness when enabling.

Spec: `specs/p78-card-context-default-runtime-flag.spec.md`

## Steps

- [x] Add P78 task contract and plan.
- [x] Add failing context tests for config default false/true and explicit disable override.
- [x] Add failing MCP context test for omitted `include_cards` using config default.
- [x] Add failing Phase-3 default-control enable/disable tests.
- [x] Add `context.include_cards_default` config load/save support.
- [x] Wire CLI and MCP context default resolution.
- [x] Implement `mempal phase3 default-control card-context` enable/disable.
- [x] Update protocol text, AGENTS/CLAUDE inventories, and MIND-MODEL summary.
- [x] Verify spec parse/lint, targeted tests, fmt/check/clippy/test, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p78-card-context-default-runtime-flag.spec.md
agent-spec lint specs/p78-card-context-default-runtime-flag.spec.md --min-score 0.7
cargo test --test context_assembler test_cli_context_config_default_false_omits_cards
cargo test --test context_assembler test_cli_context_config_default_true_includes_cards
cargo test --test context_assembler test_cli_context_no_include_cards_overrides_config_default_true
cargo test mcp::server::tests::test_mcp_context_include_cards_omitted_uses_config_default --lib
cargo test --test phase3_runtime test_cli_phase3_default_control_enable_requires_ready_proposal
cargo test --test phase3_runtime test_cli_phase3_default_control_disable_is_reversible
rg -n "p78-card-context-default-runtime-flag|P78 card context default runtime flag" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
git diff --check
```
