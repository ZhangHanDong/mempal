spec: task
name: "P103: adoption analytics"
inherits: project
tags: [phase3, adoption, analytics, phase-5]
---

## Intent

P103 adds an operator-facing analytics surface over runtime adoption evidence.
The system has many explicit adoption signals; operators need a compact way to
see which surfaces are useful, risky, or missing evidence before changing
defaults or workflows.

## Decisions

- Add CLI `mempal phase3 adoption analytics --format plain|json`.
- Add MCP action `mempal_phase3 action=analytics`.
- Analytics groups events by `track` and `feature`.
- Each group reports total, accepted, rejected, misses, rollbacks,
  contradictions, and a deterministic recommendation.
- Analytics is read-only and does not write adoption events.

## Boundaries

### Allowed Changes
- specs/p103-adoption-analytics.spec.md
- docs/plans/2026-05-26-p103-adoption-analytics.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not change promotion gates.
- Do not change card context defaults.
- Do not mutate runtime adoption events.

## Acceptance Criteria

Scenario: CLI analytics groups accepted and rejected evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_analytics_json
    Targets: `phase3 adoption analytics`.
  Given runtime adoption events exist for two features
  When `mempal phase3 adoption analytics --format json` runs
  Then JSON reports `writes=false`
  And contains grouped feature analytics
  And contains deterministic recommendations

Scenario: CLI analytics plain output is compact
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_analytics_plain
    Targets: plain output.
  When `phase3 adoption analytics --format plain` runs
  Then stdout contains `adoption analytics`
  And stdout includes feature-level counts

Scenario: MCP analytics mirrors CLI report shape
  Test:
    Filter: test_mcp_phase3_adoption_analytics_action
    Targets: `mempal_phase3 action=analytics`.
  When MCP action `analytics` runs
  Then the response includes an analytics report
  And no DB mutations occur

## Out of Scope

- Graph rendering.
- Threshold tuning.
- Automatic default changes.
