spec: task
name: "P45: card linked-evidence retrieval"
inherits: project
tags: [rust, cli, mcp, mind-model, knowledge-card]
---

## Intent

P43 defined the Phase-2 card retrieval result shape and P44 added opt-in card-aware
context assembly. P45 chooses the first explicit card retrieval strategy:
retrieve active knowledge cards through their linked evidence drawers, reusing the
existing drawer BM25+vector search path instead of adding card embeddings.

## Decisions

- Retrieval strategy is linked-evidence-first: search evidence drawers, then return active cards linked to matched evidence.
- Active cards are `promoted` and `canonical`; `candidate`, `demoted`, and `retired` are excluded by default.
- Add explicit CLI surface `mempal knowledge-card retrieve <query>`.
- Add MCP surface through `mempal_knowledge_cards` with `action="retrieve"`.
- Retrieval results include the card plus matched evidence citations with `evidence_drawer_id`, `role`, `source_file`, and score.
- Do not change `mempal_search` default behavior and do not add card embedding storage.

## Boundaries

### Allowed Changes
- src/core/db.rs
- src/core/protocol.rs
- src/core/types.rs
- src/knowledge_card_retrieval.rs
- src/lib.rs
- src/main.rs
- src/mcp/server.rs
- src/mcp/tools.rs
- tests/knowledge_card_retrieval.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- docs/plans/**

### Forbidden
- Do not add new runtime dependencies.
- Do not add card vector tables or card embedding writes.
- Do not make `mempal_search` return cards.
- Do not make card-aware context the default.

## Acceptance Criteria

Scenario: CLI retrieve returns active cards through linked evidence
  Test: test_cli_knowledge_card_retrieve_json_returns_active_card
  Given an evidence drawer that matches the query
  And a promoted card linked to that evidence drawer
  When running `mempal knowledge-card retrieve <query> --format json`
  Then the JSON result contains the promoted card
  And the result contains an evidence citation with `evidence_drawer_id`, `role`, `source_file`, and score

Scenario: CLI retrieve excludes inactive cards
  Test: test_card_retrieve_excludes_candidate_cards
  Given matching evidence linked to both promoted and candidate cards
  When retrieving cards
  Then only the promoted card is returned

Scenario: MCP retrieve action returns the same shape
  Test: test_mcp_knowledge_cards_retrieve_action
  Given the MCP entry point in `src/mcp/server.rs`
  And an active card linked to matching evidence
  And the action value is `retrieve`
  When calling `mempal_knowledge_cards` with `action="retrieve"`
  Then the response contains retrieved card results with evidence citations

Scenario: Retrieval does not mutate storage
  Test: test_card_retrieve_has_no_db_side_effects
  Given existing cards and evidence links
  When retrieving cards
  Then drawer, card, link, and event counts are unchanged

Scenario: Retrieval does not alter default search behavior
  Test: test_card_retrieve_does_not_change_mempal_search
  Given a card linked to matching evidence
  When running normal drawer search
  Then results contain drawer ids only and no knowledge card id

Scenario: Invalid retrieve top_k is rejected
  Test: test_cli_knowledge_card_retrieve_rejects_zero_top_k
  Given the CLI entry point in `src/main.rs`
  When running `mempal knowledge-card retrieve <query> --top-k 0`
  Then the command fails
  And stderr says `--top-k must be greater than 0`

## Out of Scope

- Card-level vector embeddings.
- Returning cards from default `mempal_search`.
- Making card-aware context default.
- Automatic promotion or lifecycle mutation.
