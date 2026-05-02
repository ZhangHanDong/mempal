spec: task
name: "P59: research adapter ingestion contract"
inherits: project
tags: [phase-3, research, adapter]
---

## Intent

P59 implements the first research adapter contract surface. External reports
can be validated and planned, but validation does not ingest or promote
knowledge.

## Decisions

- Add `mempal phase3 research-validate-plan`.
- The accepted input is JSON with `report_id`, `title`, `sources`, `findings`, and optional `candidate_insights`.
- Validation returns counts and errors in plain or JSON output.
- Research adapter work preserves P49 evidence-first boundaries.

## Boundaries

### Allowed Changes
- src/main.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not ingest research reports automatically.
- Do not create `dao_tian`, canonical, or promoted knowledge.
- Do not bypass distill or lifecycle gates.

## Acceptance Criteria

Scenario: valid research report is accepted for planning
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_validate_plan
  Given a valid research report JSON file
  When running `mempal phase3 research-validate-plan`
  Then the report is marked valid
  And source/finding/candidate counts are returned

Scenario: invalid research report reports missing fields
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_research_validate_plan_reports_missing_fields
  Given an invalid research report JSON file
  When validating the plan
  Then the command succeeds with `valid=false`
  And missing fields are reported

## Out of Scope

- Research ingestion execution.
- Research-driven promotion.
