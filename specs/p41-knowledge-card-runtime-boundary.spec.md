spec: task
name: "P41: knowledge card runtime boundary"
inherits: project
tags: [memory, mind-model, knowledge-cards, runtime, phase-2]
---

## Intent

P41 updates the runtime boundary documentation after Phase-2 cards gain gate and
lifecycle operations. Runtime context and search remain Stage-1 drawer based
until a dedicated card retrieval strategy is specified.

## Decisions

- Document that Phase-2 cards are governed objects, not yet the default context source.
- Keep `mempal context` and `mempal_context` on Stage-1 typed drawers in P41.
- Keep search results drawer/citation based in P41.
- Add P41 inventory entries.

## Acceptance Criteria

Scenario: MIND-MODEL design states current Phase-2 runtime boundary
  Test:
    Package: mempal
    Filter: rg -n "Phase-2 cards are governed objects" docs/MIND-MODEL-DESIGN.md
  Given MIND-MODEL-DESIGN
  When reading the Phase 2 section
  Then it states that cards are governed objects but not default context/search source

Scenario: inventories include P41
  Test:
    Package: mempal
    Filter: rg -n "p41-knowledge-card-runtime-boundary" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P41
  Then both inventories list it as completed

## Out of Scope

- Do not change context assembly.
- Do not change search.
- Do not add card embeddings.

## Constraints

- Documentation must not imply automatic runtime use of card records.
