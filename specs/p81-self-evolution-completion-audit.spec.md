spec: task
name: "P81: self-evolution completion audit"
inherits: project
tags: [phase-3, completion, audit, self-evolution]
---

## Intent

P80 resolved autonomous promotion as an explicit out-of-scope boundary, leaving
the active objective to be audited under the governed human-gated definition of
a complete self-evolving agent system. P81 performs that final completion audit:
restate the objective as concrete deliverables, map each deliverable to actual
artifacts and verification evidence, identify uncovered requirements, and state
whether the objective is complete under the current design boundary.

## Decisions

- P81 must leave its own spec and plan per the P76 invariant.
- The audited objective is `完整自进化 agent 系统`.
- "Complete" means complete under the P80 governed boundary: autonomous evidence
  capture/proposal/retrieval/adoption support is allowed, but durable lifecycle
  mutation remains human/operator-triggered.
- The audit must not rely on green CI alone; it must map concrete requirements
  to artifacts, tests, specs, protocol, and merged PR/main CI evidence.
- If fully autonomous lifecycle mutation is required, the audit must mark the
  objective incomplete and identify that as a separate future requirement.
- P81 is docs/audit only and must not implement new runtime behavior.

## Boundaries

### Allowed Changes

- specs/p81-self-evolution-completion-audit.spec.md
- docs/plans/2026-05-13-p81-self-evolution-completion-audit.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md

### Forbidden

- Do not change Rust runtime code.
- Do not change MCP protocol behavior.
- Do not change schema or tests.
- Do not mark completion without prompt-to-artifact evidence.
- Do not redefine autonomous promotion back into scope.

## Acceptance Criteria

Scenario: P81 spec and plan are present
  Test:
    Filter: agent-spec parse specs/p81-self-evolution-completion-audit.spec.md && agent-spec lint specs/p81-self-evolution-completion-audit.spec.md --min-score 0.7
    Targets: P76 spec completeness invariant.
  Given P81 is a numbered P
  When checking repository artifacts
  Then `specs/p81-self-evolution-completion-audit.spec.md` exists
  And `docs/plans/2026-05-13-p81-self-evolution-completion-audit.md` exists

Scenario: MIND-MODEL contains the final objective restatement
  Test:
    Filter: rg -n "P81 self-evolution completion audit|Objective restatement|governed human-gated complete self-evolving agent system" docs/MIND-MODEL-DESIGN.md
    Targets: Completion audit narrative.
  Given P81 audits the active objective
  When reading `docs/MIND-MODEL-DESIGN.md`
  Then it restates `完整自进化 agent 系统` as concrete deliverables
  And it names the governed human-gated boundary from P80

Scenario: MIND-MODEL maps prompts to artifacts and evidence
  Test:
    Filter: rg -n "Prompt-to-artifact checklist|Evidence substrate|Knowledge governance|Runtime feedback loop|Rollback and default control|Lifecycle authority boundary|Mainline verification" docs/MIND-MODEL-DESIGN.md
    Targets: Audit checklist.
  Given completion cannot rely on CI alone
  When reading the P81 section
  Then each deliverable is mapped to concrete specs, files, tests, or PR/CI evidence
  And the audit mentions mainline verification evidence

Scenario: MIND-MODEL states completion result and residual boundary
  Test:
    Filter: rg -n "P81 conclusion: complete|Residual boundary|fully autonomous lifecycle mutation remains out of scope" docs/MIND-MODEL-DESIGN.md
    Targets: Completion conclusion.
  Given P80 excludes autonomous lifecycle mutation
  When reading the P81 conclusion
  Then the objective is marked complete under the governed boundary
  And fully autonomous lifecycle mutation remains explicitly out of scope

Scenario: AGENTS and CLAUDE inventories include P81
  Test:
    Filter: rg -n "p81-self-evolution-completion-audit|P81 self-evolution completion audit" AGENTS.md CLAUDE.md
    Targets: Repository agent inventories.
  Given repository-level agent instructions list completed specs and plans
  When checking `AGENTS.md` and `CLAUDE.md`
  Then both files list the P81 spec
  And both files list the P81 plan

Scenario: Audit-only boundary is preserved
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: Changed file boundary.
  Given P81 is docs/audit only
  When inspecting changed paths
  Then changed files are limited to P81 spec, P81 plan, AGENTS, CLAUDE, and MIND-MODEL

## Out of Scope

- New runtime features.
- New tests or schema.
- Autonomous lifecycle mutation.
- Updating goal status before the audit PR lands on main and CI is green.
