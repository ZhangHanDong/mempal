# P74 Card Context Default-On Proposal

Goal: add a read-only proposal artifact for card-context default-on readiness
that combines P66 readiness with explicit rollback criteria while keeping
`include_cards` opt-in.

Spec: `specs/p74-card-context-default-proposal.spec.md`

## Steps

- [x] Add P74 task contract and plan.
- [x] Add failing CLI tests for ready proposal, missing rollback criteria,
  missing readiness, context default unchanged, and unknown candidate.
- [x] Add failing MCP test for read-only `default_proposal`.
- [x] Implement pure default proposal policy in `src/core/phase3.rs`.
- [x] Wire CLI `mempal phase3 default-proposal card-context`.
- [x] Wire MCP `mempal_phase3 action=default_proposal`.
- [x] Update protocol/tool descriptions and inventories.
- [x] Verify spec parse/lint, targeted tests, formatting, clippy, full tests,
  and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p74-card-context-default-proposal.spec.md
agent-spec lint specs/p74-card-context-default-proposal.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_default_proposal_card_context_ready
cargo test --test phase3_runtime test_cli_phase3_default_proposal_requires_rollback_criteria
cargo test --test phase3_runtime test_cli_phase3_default_proposal_blocks_without_readiness
cargo test --test phase3_runtime test_cli_phase3_default_proposal_keeps_context_cards_opt_in
cargo test --test phase3_runtime test_cli_phase3_default_proposal_rejects_unknown_candidate
cargo test mcp::server::tests::test_mcp_phase3_default_proposal_card_context_is_read_only --lib
rg -n "p74-card-context-default-proposal|P74 card context default-on proposal" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy -- -D warnings
cargo clippy --features rest -- -D warnings
cargo test
git diff --check
```
