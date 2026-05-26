spec: task
name: "P83: cognitive brief"
inherits: project
tags: [phase-3, cognitive-brief, synthesis, cli, evidence]
---

## Intent

P83 adds a deterministic `mempal brief` surface that moves one step beyond
retrieval: it assembles a cited, uncertainty-aware cognitive brief for a query.
The brief is not an LLM answer and must not invent facts; it organizes existing
context, evidence, cards, unresolved cues, uncertainty signals, and suggested
next actions into a compact report that an agent or human can act on.

## Decisions

- Add top-level CLI `mempal brief <query>`.
- The brief reuses existing context assembly semantics rather than creating a
  new retrieval stack.
- The brief includes `key_facts`, `evidence`, `cards`, `entities`,
  `unresolved_items`, `uncertainty`, and `next_actions`.
- Every fact, evidence item, card, unresolved item, and uncertainty item with a
  source must include drawer/source citation metadata.
- The brief is deterministic and does not call an LLM.
- The brief is read-only and must not write drawers, cards, runtime adoption
  events, config, or audit entries.
- The CLI supports `--format plain|json`, `--domain`, `--field`, `--cwd`,
  `--max-items`, and `--dao-tian-limit`.
- The default brief includes evidence and cards, because the purpose is a
  synthesis-oriented report rather than a narrow runtime context pack.
- P83 must leave its own spec and plan per the P76 invariant.

## Boundaries

### Allowed Changes
- specs/p83-cognitive-brief.spec.md
- docs/plans/2026-05-25-p83-cognitive-brief.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/brief.rs
- src/lib.rs
- src/main.rs
- src/core/protocol.rs
- tests/cognitive_brief.rs

### Forbidden
- Do not add an LLM dependency or network synthesis call.
- Do not add MCP `mempal_brief`; P83 is CLI-only.
- Do not add schema migrations.
- Do not mutate any database state while generating a brief.
- Do not change existing `mempal search`, `mempal context`, card retrieval, or
  Phase-3 adoption semantics.
- Do not claim uncertainty is resolved when supporting evidence is absent.
- Do not auto-promote, demote, distill, ingest, or record runtime adoption
  evidence from brief generation.

## Acceptance Criteria

Scenario: CLI brief JSON assembles cited cognitive report
  Test:
    Filter: cargo test --test cognitive_brief test_cli_brief_json_includes_citations_uncertainty_and_actions
    Targets: CLI `mempal brief` JSON behavior and citation shape.
  Given a test palace with cited knowledge, evidence, and one active linked card whose text contains `Alice pricing`
  When running `mempal brief "Alice pricing" --format json`
  Then stdout is valid JSON
  And it includes `key_facts`, `evidence`, `cards`, `uncertainty`, and `next_actions`
  And key facts cite `drawer_id` and `source_file`
  And card entries cite `card_id` plus linked evidence
  And no runtime adoption event or other database row is written

Scenario: CLI brief plain output is human-readable and citation-first
  Test:
    Filter: cargo test --test cognitive_brief test_cli_brief_plain_lists_sections_and_citations
    Targets: CLI plain renderer.
  Given the same cited brief fixture
  When running `mempal brief "Alice pricing"`
  Then stdout contains `## Summary`, `## Key Facts`, `## Evidence`, `## Uncertainty`, and `## Next Actions`
  And stdout contains drawer and source citations

Scenario: CLI brief no-evidence case surfaces uncertainty instead of hallucinating
  Test:
    Filter: cargo test --test cognitive_brief test_cli_brief_no_evidence_reports_uncertainty
    Targets: Empty-result behavior.
  Given an empty palace
  When running `mempal brief "Unknown account" --format json`
  Then stdout reports zero evidence items
  And uncertainty includes a `no_evidence` item
  And next actions suggest ingesting or adding evidence

Scenario: CLI brief rejects unsupported format
  Test:
    Filter: cargo test --test cognitive_brief test_cli_brief_rejects_invalid_format
    Targets: CLI error handling.
  Given the brief command
  When running `mempal brief "Alice pricing" --format yaml`
  Then the command fails
  And stderr contains `unsupported brief format`

Scenario: Protocol and inventories include P83
  Test:
    Filter: rg -n "p83-cognitive-brief|P83 cognitive brief|mempal brief" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
    Targets: Project inventories, design document, and embedded protocol.
  Given project documentation and protocol instructions
  When searching for P83 and the brief command
  Then the P83 spec, plan, design summary, and protocol guidance are recorded

## Out of Scope

- LLM-generated prose synthesis.
- MCP brief tool.
- Browser/UI rendering.
- Automatic dream-cycle maintenance.
- Entity graph extraction or KG mutation.
- Fact-check execution inside brief generation.
