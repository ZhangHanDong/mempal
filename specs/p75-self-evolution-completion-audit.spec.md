spec: task
name: "P75: self-evolution completion audit"
inherits: project
tags: [phase-3, self-evolution, audit]
---

## Intent

P70 audited the self-evolution objective before P71-P74 existed. P75 performs a
new evidence-based completion audit after the replay, capture helper, evaluator
advisory API, and card-context default proposal have landed on main. The audit
must distinguish completed governed self-evolution substrate from any remaining
requirements for a complete autonomous self-evolving agent system.

## Decisions

- P75 must leave its own `specs/p75-*.spec.md` and plan document.
- P75 is audit-only and must not change Rust runtime behavior.
- The audit must restate the objective as concrete deliverables.
- The audit must include a prompt-to-artifact checklist mapping deliverables to
  specs, commands, tests, protocol entries, and PR/CI evidence.
- The audit must explicitly state whether the full objective is complete.
- If remaining gaps exist, the audit must list next P candidates instead of
  marking the goal complete.

## Boundaries

### Allowed Changes
- specs/p75-self-evolution-completion-audit.spec.md
- docs/plans/2026-05-13-p75-self-evolution-completion-audit.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not modify `tests/**`.
- Do not add CLI or MCP behavior.
- Do not mark the overall self-evolution objective complete unless the audit
  proves no required work remains.

## Acceptance Criteria

Scenario: MIND-MODEL records P75 completion audit
  Test:
    Filter: rg -n "P75 self-evolution completion audit|P75 conclusion" docs/MIND-MODEL-DESIGN.md
    Targets: P75 audit section.
  Given the MIND-MODEL design document
  When reading the Phase-3 completion section
  Then it includes a P75 audit after P71-P74
  And it states a P75 conclusion

Scenario: Audit maps objective to concrete artifacts
  Test:
    Filter: rg -n "P75 prompt-to-artifact checklist|P71.*phase3_self_evolution_replay|P72.*capture|P73.*evaluator_advise|P74.*default_proposal" docs/MIND-MODEL-DESIGN.md
    Targets: Prompt-to-artifact checklist.
  Given the P75 audit
  When reading the checklist
  Then it maps replay, capture, evaluator advice, and default proposal to concrete artifacts

Scenario: Audit identifies remaining gaps instead of overclaiming completion
  Test:
    Filter: rg -n "P75 conclusion: not complete|Remaining gaps after P75|no automatic live tool instrumentation|no rollback executor|no actual default-on runtime change" docs/MIND-MODEL-DESIGN.md
    Targets: Completion conclusion.
  Given the P75 audit
  When reading the conclusion
  Then it states the full objective is not complete yet
  And it lists concrete remaining gaps

Scenario: Inventories include P75
  Test:
    Filter: rg -n "p75-self-evolution-completion-audit|P75 self-evolution completion audit" AGENTS.md CLAUDE.md
    Targets: Agent inventories.
  Given project inventories
  When searching for P75
  Then both AGENTS.md and CLAUDE.md include the P75 spec and plan entries

Scenario: P75 remains audit-only
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: P75 audit-only boundary.
  Given the P75 branch
  When listing changed files
  Then changes are limited to the P75 spec, plan, MIND-MODEL design, and agent inventory docs

## Out of Scope

- Implementing automatic live tool instrumentation.
- Implementing rollback execution.
- Turning `include_cards` on by default.
- Adding autonomous promotion authority.
