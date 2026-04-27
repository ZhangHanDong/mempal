spec: task
name: "P32: implement Phase-2 knowledge card schema v8"
inherits: project
tags: [memory, mind-model, knowledge-cards, schema, migration]
---

## Intent

P31 defined the Phase-2 knowledge card schema contract. P32 implements that
contract as schema v8 by adding `knowledge_cards`, `knowledge_evidence_links`,
and `knowledge_events` tables to the existing SQLite `palace.db`.

## Decisions

- Bump `CURRENT_SCHEMA_VERSION` from 7 to 8.
- Add a v8 migration that creates `knowledge_cards`,
  `knowledge_evidence_links`, and `knowledge_events`.
- Keep the migration additive and backward-compatible: existing drawers,
  triples, tunnels, vectors, and taxonomy rows must be preserved.
- Enable SQLite foreign-key enforcement on every `Database::open` connection.
- `knowledge_evidence_links.evidence_drawer_id` references `drawers(id)`.
- `knowledge_evidence_links.card_id` and `knowledge_events.card_id` reference
  `knowledge_cards(id)`.
- Enforce `UNIQUE(card_id, evidence_drawer_id, role)` on evidence links.
- Enforce CHECK constraints for tier, status, domain, anchor_kind, link role,
  and event_type.
- Keep `knowledge_events` append-only by rejecting UPDATE and DELETE through
  triggers.
- Do not add Rust `KnowledgeCard` domain structs yet.
- Do not add CLI, MCP, or REST surfaces yet.
- Do not migrate existing `memory_kind=knowledge` drawers into cards yet.

## Acceptance Criteria

Scenario: current databases open at schema v8
  Test:
    Package: mempal
    Filter: test_new_database_schema_version_is_8
  Given a new palace database
  When opening it through `Database::open`
  Then `schema_version == 8`
  And all three knowledge card tables exist

Scenario: v7 database migrates to v8 without data loss
  Test:
    Package: mempal
    Filter: test_migration_v7_to_v8_adds_knowledge_card_tables_without_data_loss
  Given a schema v7 palace with existing drawers, triples, taxonomy, and tunnels
  When opening it through `Database::open`
  Then `schema_version == 8`
  And the existing row counts are unchanged
  And all three knowledge card tables are empty

Scenario: knowledge_cards enforce enum checks
  Test:
    Package: mempal
    Filter: test_knowledge_cards_reject_invalid_tier_status_domain_anchor
  Given schema v8
  When inserting a knowledge card with invalid tier, status, domain, or anchor_kind
  Then SQLite rejects the row through CHECK constraints

Scenario: evidence links enforce drawer foreign keys and role checks
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_links_reject_invalid_role_and_missing_drawer
  Given schema v8 with one card and no matching evidence drawer
  When inserting an evidence link with `role='invalid_role'`
  Then SQLite rejects the row through a CHECK constraint
  When inserting an evidence link to a missing `evidence_drawer_id`
  Then SQLite rejects the row through a foreign-key constraint

Scenario: evidence links deduplicate per role
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_links_dedup_card_drawer_role
  Given schema v8 with one card and one evidence drawer
  When inserting the same `(card_id, evidence_drawer_id, role)` twice
  Then the second insert fails through a UNIQUE constraint

Scenario: events enforce event type and card foreign key
  Test:
    Package: mempal
    Filter: test_knowledge_events_reject_invalid_type_and_missing_card
  Given schema v8
  When inserting an event with `event_type='invalid_event'`
  Then SQLite rejects the row through a CHECK constraint
  When inserting an event for a missing card
  Then SQLite rejects the row through a foreign-key constraint

Scenario: knowledge events are append-only
  Test:
    Package: mempal
    Filter: test_knowledge_events_are_append_only
  Given schema v8 with one card and one event
  When updating or deleting the event row
  Then SQLite rejects both mutations

Scenario: indexes exist for Phase-2 query paths
  Test:
    Package: mempal
    Filter: test_knowledge_card_schema_indexes_exist
  Given schema v8
  When inspecting sqlite indexes
  Then card tier/status, domain/field, and anchor indexes exist
  And evidence link card/evidence indexes exist
  And event card/created_at index exists

## Out of Scope

- Do not add DB CRUD APIs beyond migration support.
- Do not add Rust domain structs for knowledge cards.
- Do not add CLI commands.
- Do not add MCP tools.
- Do not add REST endpoints.
- Do not change search, context, wake-up, lifecycle, or promotion-gate behavior.
- Do not backfill existing knowledge drawers into cards.

## Constraints

- Existing schemas v1-v7 must still migrate atomically to the latest version.
- The v8 migration must be additive and must not rewrite existing drawer rows.
- Foreign keys must be enforced for `Database::open` connections.
