spec: task
name: "P57: card embedding evidence gate"
inherits: project
tags: [phase-3, card-embedding, gate]
---

## Intent

P57 implements a read-only evidence gate for card embeddings. Card embeddings
remain blocked until adoption evidence shows repeated linked-evidence retrieval
misses.

## Decisions

- Add `mempal phase3 gate card-embeddings`.
- The gate is read-only and does not add vector tables.
- The gate requires at least three `card_embedding` miss signals and zero rollback signals.
- Linked evidence remains the citation root.

## Boundaries

### Allowed Changes
- src/main.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add `knowledge_card_vectors`.
- Do not change retrieval ranking.
- Do not change `mempal_search`.

## Acceptance Criteria

Scenario: card embedding gate blocks without miss evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_gate_blocks_card_embeddings_without_miss_evidence
  Given no card embedding miss evidence
  When evaluating `card-embeddings`
  Then the gate reports `ready=false`

Scenario: no card vector schema exists
  Test:
    Filter: "! rg -n \"knowledge_card_vectors|card_vectors\" src"
  Given P57 implementation
  When searching source
  Then no card vector table exists

## Out of Scope

- Card embedding implementation.
- Card reindexing.
