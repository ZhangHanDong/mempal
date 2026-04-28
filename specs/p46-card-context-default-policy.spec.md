spec: task
name: "P46: card context default policy"
inherits: project
tags: [mind-model, knowledge-card, runtime-policy]
---

## Intent

P44 made card-aware context available as an explicit opt-in and P45 added
linked-evidence card retrieval. P46 resolves the next MIND-MODEL decision point:
card-aware context must not become default yet; future default enablement requires
explicit runtime evidence and a separate implementation spec.

## Decisions

- Keep `mempal context` default drawer-only.
- Keep `mempal_context` default `include_cards=false`.
- Card-aware context remains opt-in through `--include-cards` or `include_cards=true`.
- Future default enablement requires evidence that cards improve runtime behavior without citation loss or context bloat.
- Default enablement must be a separate spec and must include rollback criteria.
- P46 is policy-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p46-card-context-default-policy.spec.md
- docs/plans/2026-04-28-p46-card-context-default-policy.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not change CLI or MCP defaults.
- Do not change `mempal_search`.
- Do not add migrations or schema changes.

## Acceptance Criteria

Scenario: MIND-MODEL records the default-context decision
  Test:
    Filter: rg -n "P46 keeps card-aware context opt-in|default context remains drawer-only" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 card retrieval/runtime section
  Then it states that P46 keeps card-aware context opt-in
  And it states that default context remains drawer-only

Scenario: MIND-MODEL defines future default evidence requirements
  Test:
    Filter: rg -n "Evidence required before default enablement|rollback criteria|context bloat" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the future default policy
  Then it lists evidence required before default enablement
  And it mentions rollback criteria

Scenario: Inventories include P46
  Test:
    Filter: rg -n "p46-card-context-default-policy|P46 card context default policy" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P46
  Then both AGENTS.md and CLAUDE.md include the P46 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P46 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Explicit opt-in language remains present
  Test:
    Filter: rg -n "include_cards=true|--include-cards|opt-in" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the card-aware context section
  Then explicit opt-in language remains present

## Out of Scope

- Changing `mempal context` or `mempal_context` defaults.
- Adding telemetry or evaluator scoring.
- Adding card embeddings.
- Returning cards from `mempal_search`.
