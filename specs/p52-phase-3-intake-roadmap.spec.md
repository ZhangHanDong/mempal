spec: task
name: "P52: phase 3 intake roadmap"
inherits: project
tags: [mind-model, phase-3, roadmap]
---

## Intent

P51 closed the P12-P50 MIND-MODEL baseline. P52 defines the next-stage intake
roadmap so future work starts from evidence-backed specs instead of reopening
the completed baseline.

## Decisions

- Phase 3 is not a continuation of unfinished P12-P50 baseline work.
- Phase 3 candidate tracks are evaluator APIs, card retrieval maturity, research
  adapter ingestion, and runtime adoption evidence.
- A Phase 3 candidate must state evidence, rollback criteria, and acceptance
  checks before implementation begins.
- Default-enabling card context or card embeddings requires measured retrieval
  benefit and rollback criteria.
- Evaluator APIs must preserve the P50 advisory-only lifecycle boundary.
- Research adapters must preserve the P49 evidence-first ingestion boundary.
- P52 is roadmap-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p52-phase-3-intake-roadmap.spec.md
- docs/plans/2026-05-01-p52-phase-3-intake-roadmap.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not implement evaluator APIs.
- Do not implement card embeddings.
- Do not default-enable card context.
- Do not implement a research adapter.
- Do not reopen P12-P50 baseline tasks.

## Acceptance Criteria

Scenario: MIND-MODEL defines Phase 3 as new-stage work
  Test:
    Filter: rg -n "P52 Phase-3 intake roadmap|Phase 3 is new-stage work, not unfinished P12-P50 baseline work" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the post-baseline roadmap
  Then it states Phase 3 is new-stage work
  And it does not reopen the completed P12-P50 baseline

Scenario: MIND-MODEL lists candidate tracks
  Test:
    Filter: rg -n "evaluator APIs|card retrieval maturity|research adapter ingestion|runtime adoption evidence" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading Phase 3 candidate tracks
  Then evaluator APIs, card retrieval maturity, research adapter ingestion, and runtime adoption evidence are listed

Scenario: MIND-MODEL requires evidence before implementation
  Test:
    Filter: rg -n "must state evidence, rollback criteria, and acceptance checks before implementation begins|default-enabling card context or card embeddings requires measured retrieval benefit" docs/MIND-MODEL-DESIGN.md
  Given the Phase 3 intake rules
  When evaluating a candidate
  Then it must define evidence, rollback, and acceptance checks before implementation
  And card defaults or embeddings require measured retrieval benefit

Scenario: Existing P49/P50 boundaries remain binding
  Test:
    Filter: rg -n "Evaluator APIs must preserve the P50 advisory-only lifecycle boundary|Research adapters must preserve the P49 evidence-first ingestion boundary" docs/MIND-MODEL-DESIGN.md
  Given Phase 3 candidate tracks
  When evaluator or research work is proposed
  Then P50 and P49 boundaries remain binding

Scenario: Inventories include P52
  Test:
    Filter: rg -n "p52-phase-3-intake-roadmap|P52 phase 3 intake roadmap" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P52
  Then both AGENTS.md and CLAUDE.md include the P52 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P52 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

## Out of Scope

- Implementing Phase 3 features.
- Choosing one Phase 3 candidate to implement.
- Changing completed P12-P50 behavior.
- Changing promotion, research, or card runtime policy.
