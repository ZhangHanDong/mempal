spec: task
name: "P51: mind model closure audit"
inherits: project
tags: [mind-model, closure, audit]
---

## Intent

P50 closed the final explicit Future Work item left by P42. P51 records the
post-P50 closure state so future agents do not keep treating
`docs/MIND-MODEL-DESIGN.md` as an unfinished implementation plan.

## Decisions

- The MIND-MODEL baseline from P12 through P50 is complete.
- Completion means the current design decisions, governance boundaries, and
  Stage-1/Phase-2 runtime surfaces are specified and implemented where intended.
- Completion does not mean every optional future enhancement is implemented.
- Future evaluator APIs, card-level embeddings, default card context, or
  research adapters must start as new-stage specs with their own evidence.
- P51 is a closure audit only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p51-mind-model-closure-audit.spec.md
- docs/plans/2026-04-29-p51-mind-model-closure-audit.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not reopen P42 Future Work.
- Do not add runtime behavior, schema changes, or MCP tools.
- Do not change existing lifecycle thresholds or policy.

## Acceptance Criteria

Scenario: MIND-MODEL states post-P50 closure
  Test:
    Filter: rg -n "P51 closure audit: the MIND-MODEL baseline is complete|No open implementation tasks remain in the P12-P50 baseline" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the closure status
  Then it states the baseline is complete
  And it states no open P12-P50 implementation tasks remain

Scenario: MIND-MODEL distinguishes closure from optional future stages
  Test:
    Filter: rg -n "Completion does not mean every optional future enhancement is implemented|must start as new-stage specs" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the closure status
  Then it distinguishes baseline closure from optional future enhancements
  And it requires new-stage specs for future enhancements

Scenario: Inventories include P51
  Test:
    Filter: rg -n "p51-mind-model-closure-audit|P51 mind model closure audit" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P51
  Then both AGENTS.md and CLAUDE.md include the P51 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P51 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Future Work list remains closed
  Test:
    Filter: rg -n "No open Future Work remains in the P42 list|P50 closes that item as policy" docs/MIND-MODEL-DESIGN.md
  Given the P51 closure audit
  When reading the P42 future-work section
  Then the final P42 future-work item remains closed

## Out of Scope

- Implementing new runtime features.
- Designing evaluator APIs.
- Enabling card-level embeddings by default.
- Changing MIND-MODEL governance rules.
