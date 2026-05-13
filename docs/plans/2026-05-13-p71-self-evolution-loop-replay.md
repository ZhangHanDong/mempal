# P71 Self-Evolution Loop Replay

Goal: add a deterministic, test-backed replay proving that the implemented
Phase-3 pieces can form one self-evolution loop without changing runtime
defaults.

Spec: `specs/p71-self-evolution-loop-replay.spec.md`

## Steps

- [x] Add P71 task contract and plan.
- [x] Add a CLI E2E replay test for research -> evidence -> card -> context -> checked adoption.
- [x] Add invalid research no-artifact replay coverage.
- [x] Update MIND-MODEL and agent inventories with P71.
- [x] Verify spec parse/lint, targeted tests, boundary, formatting, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p71-self-evolution-loop-replay.spec.md
agent-spec lint specs/p71-self-evolution-loop-replay.spec.md --min-score 0.7
cargo test --test phase3_self_evolution_replay test_cli_self_evolution_replay_research_to_context_to_adoption
cargo test --test phase3_self_evolution_replay test_cli_self_evolution_replay_invalid_research_no_artifacts
rg -n "p71-self-evolution-loop-replay|P71 self-evolution loop replay" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
git diff --name-only main...HEAD
cargo fmt -- --check
cargo check
cargo test --test phase3_self_evolution_replay
git diff --check
```
