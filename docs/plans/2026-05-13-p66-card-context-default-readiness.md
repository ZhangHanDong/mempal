# P66 Card Context Default Readiness

Goal: add a read-only readiness report that uses existing `card_context`
runtime adoption evidence for `include_cards` to decide whether card-aware
context is eligible for a future default-on spec. This does not enable cards by
default.

Spec: `specs/p66-card-context-default-readiness.spec.md`

## Steps

- [x] Add shared readiness report types and card-context default readiness logic.
- [x] Add CLI `mempal phase3 readiness card-context-default`.
- [x] Add MCP `mempal_phase3 action=readiness`.
- [x] Update protocol, MIND-MODEL design, AGENTS, and CLAUDE docs.
- [x] Verify targeted tests, full local checks, spec parse/lint, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p66-card-context-default-readiness.spec.md
agent-spec lint specs/p66-card-context-default-readiness.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_ready
cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_blocks_without_evidence
cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_blocks_rollback
cargo test --test phase3_runtime test_cli_phase3_readiness_rejects_unknown_candidate
cargo test mcp::server::tests::test_mcp_phase3_readiness_card_context_default_is_read_only --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rest -- -D warnings
cargo test
git diff --check
```
