spec: task
name: "P54: runtime adoption evidence"
inherits: project
tags: [phase-3, runtime-adoption, schema]
---

## Intent

P54 creates the evidence substrate recommended by P53. Runtime adoption signals
are stored as first-class events so later Phase-3 decisions can depend on
measured agent behavior instead of speculation.

## Decisions

- Add schema v9 table `runtime_adoption_events`.
- Store `track`, `signal`, `feature`, optional query/context/card/evaluator/research refs, note, metadata, and timestamp.
- Add DB APIs to insert and list runtime adoption events.
- Keep events append-only by convention for Phase 3 evidence.

## Boundaries

### Allowed Changes
- src/core/db.rs
- src/core/types.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not change knowledge lifecycle promotion gates.
- Do not make card context default.
- Do not add evaluator authority.

## Acceptance Criteria

Scenario: schema v9 stores runtime adoption events
  Test:
    Filter: cargo test --test phase3_runtime test_runtime_adoption_event_roundtrip_db
  Given a new database
  When opening it through `Database::open`
  Then schema version is 9
  And a runtime adoption event can be inserted and listed

Scenario: runtime event table is documented
  Test:
    Filter: rg -n "P54 runtime adoption evidence|runtime_adoption_events|schema v9" docs/MIND-MODEL-DESIGN.md AGENTS.md CLAUDE.md
  Given project documentation
  When searching for P54
  Then schema v9 and runtime adoption events are documented

## Out of Scope

- Automatic event capture.
- Changing runtime behavior based on events.
