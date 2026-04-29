# P50 Evaluator Promotion Policy

## Goal

Close the final `docs/MIND-MODEL-DESIGN.md` future-work item by defining
evaluator-assisted promotion as advisory-only. Evaluators may help identify
evidence and risks, but deterministic gates and human review remain the
authority for lifecycle changes.

## Scope

- Add `specs/p50-evaluator-promotion-policy.spec.md`.
- Update `docs/MIND-MODEL-DESIGN.md` with the evaluator promotion policy.
- Update `AGENTS.md` and `CLAUDE.md` spec/plan inventories.
- Do not change Rust runtime code.

## Steps

- [x] Capture P50 task contract.
- [x] Document evaluator advisory boundary in MIND-MODEL design.
- [x] Update agent inventories.
- [x] Run spec and grep acceptance checks.
- [ ] Commit, ingest decision memory, push, and open PR.

## Verification

```bash
agent-spec parse specs/p50-evaluator-promotion-policy.spec.md
agent-spec lint specs/p50-evaluator-promotion-policy.spec.md --min-score 0.7
rg -n "P50 defines evaluator-assisted promotion as advisory-only|Evaluators are not lifecycle actors|must not directly mutate status" docs/MIND-MODEL-DESIGN.md
rg -n "deterministic gates remain authoritative|dao_tian.*human reviewer|evaluator-only canonization.*forbidden" docs/MIND-MODEL-DESIGN.md
rg -n "p50-evaluator-promotion-policy|P50 evaluator promotion policy" AGENTS.md CLAUDE.md
rg -n "evaluator-only.*canonization|reviewer_required=true|test_cli_knowledge_gate_requires_reviewer_for_dao_tian" specs docs tests
git diff --name-only
git diff --check
```
