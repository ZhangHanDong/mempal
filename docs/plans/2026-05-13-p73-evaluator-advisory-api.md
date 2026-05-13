# P73 Evaluator Advisory API

Goal: add a deterministic, replayable evaluator advice surface that preserves
the P50 advisory-only lifecycle boundary.

Spec: `specs/p73-evaluator-advisory-api.spec.md`

## Steps

- [x] Add P73 task contract and plan.
- [x] Add failing CLI tests for supportive advice, dao_tian human review,
  weak/risky recommendations, and missing evaluator validation.
- [x] Add failing MCP test for read-only `evaluator_advise`.
- [x] Implement pure evaluator advice policy in `src/core/phase3.rs`.
- [x] Wire CLI `mempal phase3 evaluator advise`.
- [x] Wire MCP `mempal_phase3 action=evaluator_advise`.
- [x] Update protocol/tool descriptions and inventories.
- [x] Verify spec parse/lint, targeted tests, formatting, clippy, full tests,
  and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p73-evaluator-advisory-api.spec.md
agent-spec lint specs/p73-evaluator-advisory-api.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_supportive_read_only
cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_dao_tian_requires_human_review
cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_needs_evidence_and_blocks_risk
cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_rejects_missing_evaluator
cargo test mcp::server::tests::test_mcp_phase3_evaluator_advise_action_is_read_only --lib
rg -n "p73-evaluator-advisory-api|P73 evaluator advisory API" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
git diff --check
```
