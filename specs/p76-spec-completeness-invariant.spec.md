spec: task
name: "P76: spec completeness invariant"
inherits: project
tags: [governance, specs, process]
---

## Intent

P76 codifies the project rule that every numbered P must leave an explicit task
contract before it is implemented or merged. This prevents future work from
landing as undocumented implementation drift and makes the P-series auditable as
a complete decision and verification trail.

## Decisions

- Every future numbered P must include a `specs/pNN-*.spec.md` task contract.
- Every future numbered P must include a matching `docs/plans/*pNN*.md` plan.
- Agent inventories must list completed P specs and plans after the work lands.
- This rule applies to documentation-only, audit-only, policy-only, and code
  implementation P tasks.
- P76 itself must obey the same rule by adding its own spec and plan.
- P76 is governance-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p76-spec-completeness-invariant.spec.md
- docs/plans/2026-05-13-p76-spec-completeness-invariant.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md

### Forbidden
- Do not modify `src/**`.
- Do not modify `tests/**`.
- Do not add or change CLI, MCP, REST, or database behavior.
- Do not retroactively rewrite older completed spec contents.

## Acceptance Criteria

Scenario: P76 spec exists and states the invariant
  Test:
    Filter: rg -n "Every future numbered P must include|docs/plans/.*pNN|documentation-only, audit-only, policy-only" specs/p76-spec-completeness-invariant.spec.md
    Targets: P76 task contract.
  Given the P76 task contract
  When reading its decisions
  Then it requires every future numbered P to include both a spec and a plan
  And it applies the rule to non-code P tasks too

Scenario: Project agent instructions expose the hard rule
  Test:
    Filter: rg -n "每一个 numbered P|specs/pNN-\\*\\.spec\\.md|docs/plans/.*pNN|文档-only|audit-only|policy-only" AGENTS.md CLAUDE.md
    Targets: Agent instructions.
  Given project agent instructions
  When searching the Spec system section
  Then both AGENTS.md and CLAUDE.md contain the P completeness hard rule

Scenario: Inventories include P76
  Test:
    Filter: rg -n "p76-spec-completeness-invariant|P76 spec completeness invariant" AGENTS.md CLAUDE.md
    Targets: Agent inventories.
  Given project inventories
  When searching for P76
  Then both AGENTS.md and CLAUDE.md include the P76 spec and plan entries

Scenario: MIND-MODEL records the governance decision
  Test:
    Filter: rg -n "P76 spec completeness invariant|Every P must leave a spec|future P numbering" docs/MIND-MODEL-DESIGN.md
    Targets: MIND-MODEL governance note.
  Given the MIND-MODEL design document
  When reading the Phase-3 completion tail
  Then it records the P76 governance decision
  And it updates future P numbering after reserving P76 for the invariant

Scenario: P76 remains governance-only
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: P76 governance-only boundary.
  Given the P76 branch
  When listing changed files
  Then changes are limited to the P76 spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Specless future P is rejected as incomplete
  Test:
    Filter: rg -n "不能算完成|spec-less P|missing spec|必须留下.*spec" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Future P failure rule.
  Given a future numbered P without a matching task spec
  When applying the project governance rule
  Then that spec-less P is rejected as incomplete
  And the agent must create the missing spec before implementation or merge

## Out of Scope

- Implementing live adoption instrumentation.
- Implementing card-context default-on runtime behavior.
- Implementing rollback execution.
- Changing the agent-spec CLI itself.
