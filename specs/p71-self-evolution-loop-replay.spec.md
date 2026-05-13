spec: task
name: "P71: self-evolution loop replay"
inherits: project
tags: [phase-3, self-evolution, replay, verification]
---

## Intent

P70 identified that the system has the pieces for self-evolution but lacks a
deterministic end-to-end replay proving the pieces work together. P71 adds a
test-backed replay that walks a research artifact through evidence ingestion,
evidence-backed knowledge-card promotion, card-aware context assembly, and
checked runtime adoption recording.

## Decisions

- P71 must leave its own `specs/p71-*.spec.md` and plan document.
- The replay must use public CLI surfaces rather than private DB-only setup for
  the main happy path.
- The happy path must cover research evidence creation, candidate card creation,
  supporting evidence linking, gate-enforced promotion, `mempal context
  --include-cards`, and `phase3 adoption record-checked`.
- The replay must verify persisted artifacts in SQLite after the CLI sequence.
- Invalid research input must not create drawers, cards, or adoption events.
- P71 must not add schema migrations, MCP tools, background hooks, or automatic
  runtime adoption capture.

## Boundaries

### Allowed Changes
- specs/p71-self-evolution-loop-replay.spec.md
- docs/plans/2026-05-13-p71-self-evolution-loop-replay.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- tests/phase3_self_evolution_replay.rs

### Forbidden
- Do not modify `src/**`.
- Do not change runtime defaults.
- Do not add card-level embeddings.
- Do not grant autonomous promotion authority.
- Do not mark the overall self-evolution objective complete.

## Acceptance Criteria

Scenario: CLI replay proves the self-evolution loop
  Test:
    Filter: cargo test --test phase3_self_evolution_replay test_cli_self_evolution_replay_research_to_context_to_adoption
    Targets: CLI E2E replay from research report to runtime adoption evidence.
  Given a valid research report with one finding and one candidate insight
  When the replay runs public CLI commands for research ingest, card create/link/promote, context, and checked adoption recording
  Then research evidence is persisted as an evidence drawer
  And a promoted knowledge card links to the evidence drawer as supporting and verification evidence
  And card-aware context returns the promoted card with evidence citation
  And checked runtime adoption writes one accepted `card_context/include_cards` event

Scenario: Invalid research input does not create replay artifacts
  Test:
    Filter: cargo test --test phase3_self_evolution_replay test_cli_self_evolution_replay_invalid_research_no_artifacts
    Targets: Invalid input no-write behavior.
  Given an invalid research report
  When `phase3 research-ingest-plan --execute --format json` is run
  Then the report is invalid and `writes=false`
  And no drawers, knowledge cards, or runtime adoption events are created

Scenario: P71 is recorded in project inventories
  Test:
    Filter: rg -n "p71-self-evolution-loop-replay|P71 self-evolution loop replay" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Design and agent inventory documentation.
  Given the project documentation
  When searching for P71
  Then the spec, plan, and implemented replay status are recorded

Scenario: P71 remains a replay-only change
  Test:
    Filter: git diff --name-only main...HEAD
    Targets: P71 boundary.
  Given the P71 branch
  When listing changed files
  Then changes are limited to the P71 spec, plan, docs, and replay test

## Out of Scope

- Automatic adoption capture around live agent tool execution.
- Evaluator advisory API contracts.
- Default-on card context.
- Card-level embedding schema or retrieval.
- Runtime rollback executors.
