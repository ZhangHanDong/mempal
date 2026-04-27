spec: task
name: "P38: knowledge card gate"
inherits: project
tags: [memory, mind-model, knowledge-cards, gate, phase-2]
---

## Intent

P38 adds a Phase-2 promotion gate for `knowledge_cards` using their evidence
links. This mirrors Stage-1 gate policy but reads `knowledge_evidence_links`
instead of drawer ref arrays.

## Decisions

- Add a core `evaluate_card_gate_by_id` function.
- Add CLI command `mempal knowledge-card gate`.
- Gate is read-only and must not mutate cards, links, events, drawers, vectors, or audit logs.
- Evidence counts come from `knowledge_evidence_links` grouped by role.
- Reuse the Stage-1 threshold semantics for `dao_tian`, `dao_ren`, `shu`, and `qi`.

## Acceptance Criteria

Scenario: CLI card gate counts evidence links
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_gate_counts_links
  Given one knowledge card with supporting and verification evidence links
  When running `mempal knowledge-card gate <card_id> --target-status promoted --format json`
  Then JSON reports the correct role counts
  And allowed is true when the policy threshold is satisfied

Scenario: MCP card gate is exposed through knowledge cards tool
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_gate_and_lifecycle_actions
  Given one knowledge card with insufficient evidence links
  When calling `mempal_knowledge_cards` with action `gate`
  Then the response includes a gate report
  And no card status is mutated

## Out of Scope

- Do not add automatic promotion.
- Do not change Stage-1 drawer gate behavior.
- Do not change search or context assembly.

## Constraints

- Gate failures must be observable through reasons.
- Unsupported target status values must fail deterministically.
