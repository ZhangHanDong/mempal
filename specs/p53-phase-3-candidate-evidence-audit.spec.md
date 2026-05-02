spec: task
name: "P53: phase 3 candidate evidence audit"
inherits: project
tags: [mind-model, phase-3, evidence-audit]
---

## Intent

P52 defined Phase-3 intake rules but did not choose an implementation track.
P53 audits the current evidence for each Phase-3 candidate and records the next
recommended step without implementing new runtime behavior.

## Decisions

- No Phase-3 candidate is ready for direct implementation yet.
- Runtime adoption evidence is the recommended first Phase-3 measurement track.
- Card retrieval maturity has partial evidence from P43-P45, but still needs measured retrieval misses and context impact before default changes or embeddings.
- Evaluator APIs remain blocked on concrete advisory output contracts and lifecycle replay requirements.
- Research adapter ingestion remains blocked on an explicit external report/input contract.
- P53 is audit-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p53-phase-3-candidate-evidence-audit.spec.md
- docs/plans/2026-05-02-p53-phase-3-candidate-evidence-audit.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not implement Phase-3 runtime features.
- Do not choose card context default enablement.
- Do not implement card embeddings.
- Do not implement evaluator APIs.
- Do not implement research adapter ingestion.

## Acceptance Criteria

Scenario: MIND-MODEL records candidate readiness
  Test:
    Filter: rg -n "P53 Phase-3 candidate evidence audit|No Phase-3 candidate is ready for direct implementation yet" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-3 evidence audit
  Then it states that no candidate is ready for direct implementation
  And it identifies the section as P53

Scenario: MIND-MODEL recommends the first measurement track
  Test:
    Filter: rg -n "Recommended first Phase-3 track: runtime adoption evidence|runtime adoption evidence is the common measurement substrate" docs/MIND-MODEL-DESIGN.md
  Given the candidate evidence audit
  When reading the recommendation
  Then runtime adoption evidence is recommended first
  And the reason is that it supports later candidate decisions

Scenario: MIND-MODEL records card retrieval evidence gap
  Test:
    Filter: rg -n "Card retrieval maturity: partial evidence from P43-P45|needs measured retrieval misses and context impact" docs/MIND-MODEL-DESIGN.md
  Given the card retrieval candidate
  When reading the evidence audit
  Then linked-evidence retrieval is recognized as partial evidence
  And measured misses/context impact remain required

Scenario: MIND-MODEL records evaluator and research blockers
  Test:
    Filter: rg -n "Evaluator APIs: blocked on advisory output contracts and lifecycle replay requirements|Research adapter ingestion: blocked on an explicit external report/input contract" docs/MIND-MODEL-DESIGN.md
  Given evaluator and research candidates
  When reading the evidence audit
  Then both blockers are explicitly recorded

Scenario: Inventories include P53
  Test:
    Filter: rg -n "p53-phase-3-candidate-evidence-audit|P53 phase 3 candidate evidence audit" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P53
  Then both AGENTS.md and CLAUDE.md include the P53 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P53 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

## Out of Scope

- Implementing runtime adoption measurement.
- Implementing any Phase-3 feature.
- Changing Phase-2 knowledge card behavior.
- Changing research or evaluator policy.
