# P52 Phase 3 Intake Roadmap

## Goal

Define the post-baseline intake rules for Phase 3. P12-P50 is closed; future
work must start as new-stage specs with evidence, rollback criteria, and
acceptance checks before implementation.

## Scope

- Add `specs/p52-phase-3-intake-roadmap.spec.md`.
- Add Phase-3 intake roadmap text to `docs/MIND-MODEL-DESIGN.md`.
- Update `AGENTS.md` and `CLAUDE.md` inventories.
- Do not change runtime code.

## Steps

- [x] Capture P52 task contract.
- [x] Document Phase-3 intake rules and candidate tracks.
- [x] Update agent inventories.
- [x] Run spec and grep acceptance checks.
- [ ] Commit, ingest decision memory, push, and open PR.

## Verification

```bash
agent-spec parse specs/p52-phase-3-intake-roadmap.spec.md
agent-spec lint specs/p52-phase-3-intake-roadmap.spec.md --min-score 0.7
rg -n "P52 Phase-3 intake roadmap|Phase 3 is new-stage work, not unfinished P12-P50 baseline work" docs/MIND-MODEL-DESIGN.md
rg -n "evaluator APIs|card retrieval maturity|research adapter ingestion|runtime adoption evidence" docs/MIND-MODEL-DESIGN.md
rg -n "must state evidence, rollback criteria, and acceptance checks before implementation begins|default-enabling card context or card embeddings requires measured retrieval benefit" docs/MIND-MODEL-DESIGN.md
rg -n "Evaluator APIs must preserve the P50 advisory-only lifecycle boundary|Research adapters must preserve the P49 evidence-first ingestion boundary" docs/MIND-MODEL-DESIGN.md
rg -n "p52-phase-3-intake-roadmap|P52 phase 3 intake roadmap" AGENTS.md CLAUDE.md
git diff --name-only
git diff --check
```
