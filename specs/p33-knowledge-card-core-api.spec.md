spec: task
name: "P33: knowledge card core DB API"
inherits: project
tags: [memory, mind-model, knowledge-cards, db-api, phase-2]
---

## Intent

P32 added the schema v8 tables for Phase-2 knowledge cards. P33 adds the Rust
domain types and core `Database` APIs needed to create, read, update, list, link
evidence, and append events. This remains a DB-layer feature only; no CLI, MCP,
REST, search, context, or backfill behavior is exposed yet.

## Decisions

- Add Rust types for `KnowledgeCard`, `KnowledgeEvidenceLink`, and
  `KnowledgeCardEvent`.
- Add enums for `KnowledgeEvidenceRole` and `KnowledgeEventType`.
- Reuse existing `KnowledgeTier`, `KnowledgeStatus`, `MemoryDomain`,
  `AnchorKind`, and `TriggerHints` types for card metadata.
- Add `KnowledgeCardFilter` for read/list filtering.
- Add `Database::insert_knowledge_card`.
- Add `Database::get_knowledge_card`.
- Add `Database::list_knowledge_cards`.
- Add `Database::update_knowledge_card`.
- Add `Database::insert_knowledge_evidence_link`.
- Add `Database::knowledge_evidence_links`.
- Add `Database::append_knowledge_event`.
- Add `Database::knowledge_events`.
- `insert_knowledge_evidence_link` must reject missing drawers and drawers whose
  `memory_kind != evidence`.
- `append_knowledge_event` appends rows only; existing schema triggers continue
  to reject event update/delete.
- Do not add delete APIs for cards or events in P33.
- Do not add CLI, MCP, REST, search, context, wake-up, lifecycle, promotion gate,
  or backfill behavior.

## Acceptance Criteria

Scenario: create and get knowledge card roundtrip
  Test:
    Package: mempal
    Filter: test_knowledge_card_insert_get_roundtrip
  Given schema v8
  When inserting a `KnowledgeCard` through `Database::insert_knowledge_card`
  Then `Database::get_knowledge_card` returns the same typed card
  And trigger hints and optional metadata roundtrip

Scenario: list knowledge cards supports filters
  Test:
    Package: mempal
    Filter: test_knowledge_card_list_filters
  Given schema v8 with cards across tier, status, domain, field, and anchor
  When calling `Database::list_knowledge_cards` with each filter
  Then only matching cards are returned

Scenario: update knowledge card preserves identity
  Test:
    Package: mempal
    Filter: test_knowledge_card_update_preserves_identity_and_created_at
  Given an existing knowledge card
  When updating statement, content, status, scope constraints, trigger hints, and updated_at
  Then the card id and created_at remain unchanged
  And the mutable fields are updated

Scenario: evidence link validates evidence drawer kind
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_link_requires_evidence_drawer
  Given one card, one evidence drawer, and one knowledge drawer
  When linking the evidence drawer
  Then the link succeeds
  When linking the knowledge drawer or a missing drawer
  Then the API rejects the link before storing it

Scenario: evidence links list by card
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_links_list_by_card
  Given two cards with evidence links
  When listing links for one card
  Then only that card's links are returned ordered by created_at and id

Scenario: events append and list by card
  Test:
    Package: mempal
    Filter: test_knowledge_events_append_and_list_by_card
  Given two cards with events
  When listing events for one card
  Then only that card's events are returned ordered by created_at and id
  And metadata roundtrips as JSON

Scenario: no runtime surfaces are added
  Test: docs_p33_no_runtime_surface_added
  Given P33 is DB-layer only
  When running `! rg -n "mempal_knowledge_card|KnowledgeCard" src/main.rs src/mcp src/api src/search`
  Then no user-facing knowledge card runtime is exposed

## Out of Scope

- Do not add CLI commands.
- Do not add MCP tools.
- Do not add REST endpoints.
- Do not change search result shape.
- Do not change context assembly.
- Do not change wake-up.
- Do not migrate or backfill existing `memory_kind=knowledge` drawers.
- Do not implement card deletion.
- Do not make card APIs write the existing JSONL audit log.

## Constraints

- Keep the API close to the schema names; avoid premature abstraction.
- Preserve raw evidence in `drawers`; cards only link to evidence drawer ids.
- Keep event history append-only.
- Keep the implementation inside `src/core` and tests; no runtime surface wiring.
