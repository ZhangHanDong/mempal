spec: task
name: "P50: evaluator promotion policy"
inherits: project
tags: [mind-model, evaluator, promotion-policy]
---

## Intent

P20/P27/P38 already provide deterministic promotion gates, and P49 closed the
research ingestion boundary. P50 closes the final MIND-MODEL future-work item
by defining evaluator-assisted promotion as advisory-only: evaluators may
recommend and gather evidence, but promotion remains gate-enforced and
human-reviewed for high-level knowledge.

## Decisions

- Evaluators are advisory inputs, not lifecycle actors.
- Evaluators may propose supporting, verification, teaching, and counterexample refs.
- Evaluators may produce risk notes, contradiction candidates, and promotion recommendations.
- Evaluators must not directly mutate lifecycle status, append lifecycle refs, or bypass deterministic gates.
- `dao_tian -> canonical` and any high-level knowledge requiring reviewer cannot be satisfied by evaluator-only review.
- Promotion and demotion remain through existing gate-enforced CLI/MCP lifecycle surfaces.
- P50 is policy-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p50-evaluator-promotion-policy.spec.md
- docs/plans/2026-04-29-p50-evaluator-promotion-policy.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not add evaluator scoring, telemetry, or runtime APIs.
- Do not change promotion thresholds or reviewer requirements.
- Do not add automatic promotion or demotion.

## Acceptance Criteria

Scenario: MIND-MODEL records evaluator advisory boundary
  Test:
    Filter: rg -n "P50 defines evaluator-assisted promotion as advisory-only|Evaluators are not lifecycle actors|must not directly mutate status" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading evaluator promotion policy
  Then it states evaluator assistance is advisory-only
  And it states evaluators cannot directly mutate lifecycle state

Scenario: MIND-MODEL preserves deterministic gates and human review
  Test:
    Filter: rg -n "deterministic gates remain authoritative|dao_tian.*human reviewer|evaluator-only canonization.*forbidden" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading evaluator promotion policy
  Then deterministic gates remain authoritative
  And dao_tian canonicalization still requires human review

Scenario: Inventories include P50
  Test:
    Filter: rg -n "p50-evaluator-promotion-policy|P50 evaluator promotion policy" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P50
  Then both AGENTS.md and CLAUDE.md include the P50 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P50 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Existing no evaluator-only canonization rule remains visible
  Test:
    Filter: rg -n "evaluator-only.*canonization|reviewer_required=true|test_cli_knowledge_gate_requires_reviewer_for_dao_tian" specs docs tests
  Given existing gate policy and tests
  When searching evaluator/reviewer rules
  Then existing reviewer constraints remain visible

## Out of Scope

- Implementing evaluator APIs.
- Adding evaluator scoring.
- Automatic promotion or demotion.
- Changing promotion gates.
