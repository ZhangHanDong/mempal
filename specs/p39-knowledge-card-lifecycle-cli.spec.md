spec: task
name: "P39: knowledge card lifecycle CLI"
inherits: project
tags: [memory, mind-model, knowledge-cards, lifecycle, phase-2]
---

## Intent

P39 adds governed CLI lifecycle mutation for Phase-2 knowledge cards. Promote
and demote operate on `knowledge_cards`, add role-specific evidence links, and
append `knowledge_events`.

## Decisions

- Add CLI commands `mempal knowledge-card promote` and `mempal knowledge-card demote`.
- Promotion requires at least one `--verification-ref`.
- Demotion requires at least one `--evidence-ref`, linked as counterexample evidence.
- Promotion is gate-enforced by default.
- Card status update, new evidence links, and lifecycle event append are transactional.
- Gate failure must not write links, events, or status changes.

## Acceptance Criteria

Scenario: CLI promote appends verification link and event
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_promote_gate_enforced_and_appends_event
  Given a candidate `qi` card with supporting evidence
  When promoting it with a verification evidence ref
  Then the card status becomes promoted
  And a promoted event is appended

Scenario: CLI promote gate failure is atomic
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_promote_gate_failure_does_not_mutate
  Given a card without enough supporting evidence
  When promotion gate fails
  Then card status, links, and events are unchanged

Scenario: CLI demote appends counterexample link and event
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_demote_links_counterexample_and_appends_event
  Given a promoted card
  When demoting it with counterexample evidence
  Then the card status becomes demoted
  And a demoted event is appended

## Out of Scope

- Do not delete cards.
- Do not mutate Stage-1 knowledge drawers.
- Do not write JSONL audit logs for Phase-2 card lifecycle.

## Constraints

- Evidence refs must point to existing evidence drawers.
- Lifecycle events remain append-only.
