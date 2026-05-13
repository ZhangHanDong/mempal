spec: task
name: "P80: autonomous promotion boundary audit"
inherits: project
tags: [phase-3, governance, autonomy, audit]
---

## Intent

P70 and P75 treated missing autonomous promotion authority as a possible gap in
the "complete self-evolving agent system" objective. P80 resolves that ambiguity
as a design boundary instead of adding unsafe authority: mempal may autonomously
collect evidence, propose candidates, evaluate gates, recommend actions, enable
explicit default controls, and execute explicit rollback controls, but it must
not autonomously mutate knowledge lifecycle status. Human-gated or explicit
operator-triggered lifecycle mutation is the final governance boundary for this
design stage.

## Decisions

- P80 must leave its own spec and plan per the P76 invariant.
- Autonomous promotion is explicitly out of scope for the current complete
  self-evolution design.
- `mempal_knowledge_promote`, `mempal_knowledge_demote`, knowledge-card
  promote/demote, and equivalent CLI commands remain explicit lifecycle
  mutation surfaces.
- Evaluators remain advisory-only and cannot satisfy reviewer or lifecycle
  authority requirements.
- Runtime adoption, research ingestion, card context default control, and
  rollback control may affect evidence/config, but must not bypass lifecycle
  gates or human/operator intent.
- P80 is a docs/protocol/spec audit only; it must not add schema, background
  workers, hooks, daemons, or automatic promotion execution.
- After P80, the active completion question should be reframed as: whether the
  governed self-evolution system, with human-gated lifecycle authority, satisfies
  the user's objective.

## Boundaries

### Allowed Changes

- specs/p80-autonomous-promotion-boundary-audit.spec.md
- docs/plans/2026-05-13-p80-autonomous-promotion-boundary-audit.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/core/protocol.rs

### Forbidden

- Do not implement autonomous promotion or demotion.
- Do not add new CLI or MCP lifecycle mutation commands.
- Do not change SQLite schema.
- Do not change promotion/demotion gate behavior.
- Do not add background workers, hooks, or daemons.
- Do not mark the whole active goal complete without a separate completion
  audit against current artifacts.

## Acceptance Criteria

Scenario: P80 spec and plan are present
  Test:
    Filter: agent-spec parse specs/p80-autonomous-promotion-boundary-audit.spec.md && agent-spec lint specs/p80-autonomous-promotion-boundary-audit.spec.md --min-score 0.7
    Targets: P76 spec completeness invariant.
  Given P80 is a numbered P
  When checking repository artifacts
  Then `specs/p80-autonomous-promotion-boundary-audit.spec.md` exists
  And `docs/plans/2026-05-13-p80-autonomous-promotion-boundary-audit.md` exists

Scenario: MIND-MODEL records human-gated lifecycle as final boundary
  Test:
    Filter: rg -n "P80 autonomous promotion boundary audit|human-gated lifecycle authority|autonomous promotion is out of scope" docs/MIND-MODEL-DESIGN.md
    Targets: Design narrative.
  Given P80 resolves the autonomous promotion ambiguity
  When reading `docs/MIND-MODEL-DESIGN.md`
  Then it states autonomous promotion is out of scope
  And it states human-gated lifecycle authority is the final governance boundary
  And it identifies the next step as a completion audit, not more implementation

Scenario: Protocol tells agents not to autonomously mutate lifecycle state
  Test:
    Filter: rg -n "must not autonomously promote|human/operator-triggered lifecycle mutation|evaluator advice remains advisory" src/core/protocol.rs
    Targets: MCP protocol instructions.
  Given agents consume `MEMORY_PROTOCOL`
  When reading `src/core/protocol.rs`
  Then the protocol forbids autonomous promotion
  And it keeps evaluator advice advisory-only
  And it requires explicit human/operator-triggered lifecycle mutation

Scenario: AGENTS and CLAUDE inventories include P80
  Test:
    Filter: rg -n "p80-autonomous-promotion-boundary-audit|P80 autonomous promotion boundary audit" AGENTS.md CLAUDE.md
    Targets: Repository agent inventories.
  Given repository-level agent instructions list completed specs and plans
  When checking `AGENTS.md` and `CLAUDE.md`
  Then both files list the P80 spec
  And both files list the P80 plan

Scenario: No autonomous lifecycle implementation is added
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: Implementation boundary.
  Given P80 is a docs/protocol audit
  When inspecting changed paths
  Then changed files are limited to P80 spec, P80 plan, AGENTS, CLAUDE, MIND-MODEL, and protocol docs
  And no Rust command, MCP handler, schema, or lifecycle implementation file is changed except `src/core/protocol.rs`

## Out of Scope

- Autonomous promotion implementation.
- Autonomous demotion implementation.
- Agent runtime wrappers that call lifecycle mutation commands.
- New completion claim without an explicit audit spec or audit artifact.
