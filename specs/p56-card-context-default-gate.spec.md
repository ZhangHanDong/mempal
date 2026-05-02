spec: task
name: "P56: card context default gate"
inherits: project
tags: [phase-3, card-context, gate]
---

## Intent

P56 implements the decision gate required before card-aware context can become
default. The gate reads P54/P55 adoption evidence and reports readiness without
changing defaults.

## Decisions

- Add `mempal phase3 gate card-context-default`.
- The gate is read-only.
- The gate requires at least three accepted `card_context` signals and zero rollback signals.
- Passing the gate does not change `include_cards` defaults.

## Boundaries

### Allowed Changes
- src/main.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not default-enable card-aware context.
- Do not change `mempal context` or `mempal_context` defaults.

## Acceptance Criteria

Scenario: card context gate reads adoption evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_record_stats_and_gate
  Given three accepted card context adoption events
  When evaluating `card-context-default`
  Then the gate reports `ready=true`
  And the required track is `card_context`

Scenario: card context remains explicit
  Test:
    Filter: rg -n "Card context default gate remains read-only|include_cards remains opt-in" docs/MIND-MODEL-DESIGN.md
  Given the P56 documentation
  When reading the card context gate policy
  Then the default behavior remains unchanged

## Out of Scope

- Enabling card context by default.
- Runtime telemetry capture.
