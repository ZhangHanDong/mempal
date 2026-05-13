spec: task
name: "P67: research adapter evidence ingest"
inherits: project
tags: [phase-3, research, adapter, evidence, cli]
---

## Intent

P59 validates external research report JSON, but it still stops before creating
memory evidence. P67 adds an explicit evidence-first ingest plan/apply CLI so
external research output can enter mempal as cited evidence drawers while
preserving P49: research cannot directly define dao, canonical knowledge, or
promoted knowledge.

## Decisions

- Add CLI `mempal phase3 research-ingest-plan <path>`.
- The command accepts the same JSON report contract as P59: `report_id`,
  `title`, `sources`, `findings`, and optional `candidate_insights`.
- The command defaults to dry-run and returns `writes=false`.
- `--execute` writes one `memory_kind=evidence` drawer per finding with
  `provenance=research`.
- Written evidence drawer ids are stable and idempotent; rerunning `--execute`
  skips existing drawers instead of rewriting them.
- `candidate_insights` are reported only as distill suggestions backed by the
  planned evidence drawer refs; they do not create knowledge drawers.
- The command supports `--format plain|json`, defaulting to `plain`.

## Boundaries

### Allowed Changes
- specs/p67-research-adapter-ingest.spec.md
- docs/plans/2026-05-13-p67-research-adapter-ingest.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/main.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add schema v10 or any table.
- Do not add automatic/background research ingestion.
- Do not change `mempal phase3 research-validate-plan` behavior.
- Do not create `memory_kind=knowledge` drawers from research reports.
- Do not create `dao_tian`, `canonical`, or `promoted` knowledge.
- Do not bypass `mempal knowledge distill`, lifecycle gates, or human review.
- Do not add MCP write access for research ingestion in P67.

## Acceptance Criteria

Scenario: dry-run research ingest plans evidence without writing
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_dry_run_json_no_write
    Targets: CLI JSON output and SQLite no-write side effect check.
  Given a valid research report with one finding and one candidate insight
  When running `mempal phase3 research-ingest-plan <path> --format json`
  Then the command succeeds
  And the response has `valid=true`
  And `writes=false`
  And it returns one planned evidence drawer
  And it returns one candidate insight suggestion
  And the suggestion includes a `mempal knowledge distill` command
  And the database has no drawers

Scenario: execute writes research evidence drawers idempotently
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_execute_writes_research_evidence
    Targets: CLI execution, SQLite drawer state, research provenance, and idempotency.
  Given a valid research report with two findings
  When running `mempal phase3 research-ingest-plan <path> --execute --format json`
  Then the command succeeds
  And `writes=true`
  And two evidence drawers are created
  And each created drawer has `memory_kind=evidence`
  And each created drawer has `provenance=research`
  And no knowledge drawer is created
  When running the same command again
  Then existing drawer ids are skipped and not duplicated

Scenario: invalid research ingest plan does not write
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_invalid_report_no_write
    Targets: CLI JSON output and SQLite no-write side effect check.
  Given an invalid research report JSON file
  When running `mempal phase3 research-ingest-plan <path> --execute --format json`
  Then the command succeeds with `valid=false`
  And `writes=false`
  And errors mention required fields
  And the database has no drawers

Scenario: research ingest plan rejects unsupported format
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_rejects_invalid_format
    Targets: CLI stderr and exit status.
  Given a valid research report JSON file
  When running `mempal phase3 research-ingest-plan <path> --format yaml`
  Then the command fails
  And stderr mentions `unsupported phase3 research ingest format`

## Out of Scope

- MCP research ingestion.
- Research report fetching or browser automation.
- Research-driven promotion/demotion.
- Card embeddings or default card context changes.
