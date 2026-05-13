spec: task
name: "P70: self-evolution completion audit"
inherits: project
tags: [phase-3, self-evolution, audit, mind-model]
---

## Intent

P69 made runtime adoption evidence safer to write, but the active objective is
larger than any single feature: a complete self-evolving agent system. P70 adds
an explicit completion audit that maps the objective to concrete artifacts,
identifies which loops are implemented, and records remaining gaps without
claiming the overall goal is complete.

## Decisions

- P70 is audit-only and must not change runtime behavior.
- The audit must restate "complete self-evolving agent system" as concrete
  deliverables.
- The audit must map each deliverable to existing P-level artifacts and evidence.
- The audit must explicitly state that P12-P69 do not yet prove full completion.
- The audit must list missing or weakly verified loops as future P candidates.
- Every future P still requires its own `specs/pNN-*.spec.md` and plan document.

## Boundaries

### Allowed Changes
- specs/p70-self-evolution-completion-audit.spec.md
- docs/plans/2026-05-13-p70-self-evolution-completion-audit.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not add schema migrations.
- Do not add hooks, background workers, or automatic instrumentation.
- Do not change runtime defaults.
- Do not mark the thread goal complete.
- Do not claim self-evolution is complete without listing remaining gaps.

## Acceptance Criteria

Scenario: MIND-MODEL records the P70 completion audit
  Test:
    Filter: rg -n "P70 self-evolution completion audit|Complete self-evolving agent system deliverables" docs/MIND-MODEL-DESIGN.md
    Targets: Design document audit section.
  Given the MIND-MODEL design document
  When reading the Phase-3 audit section
  Then it identifies P70 as a self-evolution completion audit
  And it restates the objective as concrete deliverables

Scenario: Audit maps implemented deliverables to artifacts
  Test:
    Filter: rg -n "Evidence substrate.*P54|Knowledge governance.*P12-P28|Knowledge cards.*P31-P45|Research ingestion.*P49/P59/P67/P68|Runtime adoption.*P54-P69" docs/MIND-MODEL-DESIGN.md
    Targets: Prompt-to-artifact checklist in MIND-MODEL.
  Given the P70 audit checklist
  When searching for implemented deliverables
  Then it maps evidence substrate, knowledge governance, cards, research, and runtime adoption to concrete P artifacts

Scenario: Audit states the objective is not complete yet
  Test:
    Filter: rg -n "P70 conclusion: not complete|Remaining gaps before full self-evolution" docs/MIND-MODEL-DESIGN.md
    Targets: Explicit non-completion statement.
  Given the P70 audit
  When reading its conclusion
  Then it states the full self-evolution objective is not complete yet
  And it lists remaining gaps

Scenario: Audit lists future P candidates
  Test:
    Filter: rg -n "P71 candidate|P72 candidate|P73 candidate" docs/MIND-MODEL-DESIGN.md
    Targets: Future work candidate list.
  Given the P70 audit
  When reading the future work list
  Then it names at least three future P candidates

Scenario: Inventories include P70
  Test:
    Filter: rg -n "p70-self-evolution-completion-audit|P70 self-evolution completion audit" AGENTS.md CLAUDE.md
    Targets: Agent inventory documents.
  Given repo agent inventories
  When searching for P70
  Then both AGENTS.md and CLAUDE.md include P70 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: P70 audit-only boundary.
  Given the P70 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

## Out of Scope

- Implementing P71 or later candidates.
- Adding automatic adoption capture.
- Turning card context on by default.
- Adding evaluator lifecycle APIs.
- Marking the overall thread goal complete.
