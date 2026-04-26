spec: task
name: "P29: wake-up and mind-model context boundary"
inherits: project
tags: [memory, wake-up, context, mind-model, protocol]
---

## Intent

`wake-up` and `mempal context` now both touch active memory, but they have
different jobs:

- `wake-up` refreshes agent identity and the most important recent drawers.
- `mempal context` assembles typed mind-model guidance in the order
  `dao_tian -> dao_ren -> shu -> qi`.

This task resolves the remaining design question by making that boundary
explicit. `wake-up` must not become a second typed context assembler.

## Decisions

- Keep `wake-up` as an L0/L1 memory refresh surface.
- Keep typed mind-model assembly exclusive to `mempal context` and
  `mempal_context`.
- `wake-up` may include knowledge drawers if they are selected by the existing
  importance-ranked top-drawer logic.
- When `wake-up` includes a knowledge drawer, it continues using the effective
  wake-up text: `statement` first, then `content` fallback.
- `wake-up` must not add tier sections such as `dao_tian`, `dao_ren`, `shu`, or
  `qi`.
- `wake-up` must not apply `dao_tian_limit` or any other tier-specific context
  budget.
- Protocol guidance must tell agents to use `wake-up` for refresh and
  `mempal_context` for typed operating guidance.
- No schema, ranking, MCP tool, REST API, or CLI flag changes.

## Acceptance Criteria

Scenario: protocol documents the boundary
  Test:
    Package: mempal
    Filter: contains_wake_up_context_boundary_guidance
  Given the embedded MEMORY_PROTOCOL
  When reading Rule 1 / Rule 3b guidance
  Then it states that wake-up is not the typed dao/shu/qi assembler
  And it directs agents to use `mempal_context` for typed operating guidance

Scenario: plain wake-up does not add mind-model sections
  Test:
    Package: mempal
    Filter: test_wake_up_does_not_assemble_mind_model_sections
  Given active `dao_tian` and `dao_ren` knowledge drawers
  When running `mempal wake-up`
  Then stdout still uses the existing L0/L1 structure
  And stdout does not contain tier section headings such as `## dao_tian` or `## dao_ren`
  And selected knowledge drawers still display their `statement`

Scenario: AAAK wake-up remains a refresh summary
  Test:
    Package: mempal
    Filter: test_wake_up_aaak_does_not_assemble_mind_model_sections
  Given active `dao_tian` and `qi` knowledge drawers
  When running `mempal wake-up --format aaak`
  Then the decoded text contains selected statement text
  And it does not introduce typed mind-model section headings

Scenario: existing wake-up ordering remains unchanged
  Test:
    Package: mempal
    Filter: test_wake_up_preserves_existing_top_drawer_order
  Given multiple drawers with different importance
  When running `mempal wake-up`
  Then the existing top-drawer ordering is preserved

## Out of Scope

- Do not change database schema.
- Do not add a new MCP tool.
- Do not add `wake-up` CLI flags.
- Do not change `top_drawers` ranking.
- Do not make `wake-up` consume `dao_tian_limit`.
- Do not move typed context assembly out of `src/context.rs`.
- Do not execute skills or tools based on wake-up output.

## Constraints

- `mempal context` / `mempal_context` remain the only Stage-1 typed
  mind-model context assemblers.
- `wake-up` remains backward-compatible with existing plain, protocol, and
  AAAK modes.
