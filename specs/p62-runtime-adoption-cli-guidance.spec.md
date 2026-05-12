spec: task
name: "P62: runtime adoption CLI guidance"
inherits: project
tags: ["phase-3", "runtime-adoption", "cli", "protocol"]
---

## Intent

P61 exposed the runtime adoption recording protocol through MCP, but the CLI
surface still lacks the same read-only guidance. P62 adds CLI parity through
`mempal phase3 adoption guidance` so humans and non-MCP agents can inspect the
same deterministic recording semantics before appending runtime evidence.

## Decisions

- Add `mempal phase3 adoption guidance`.
- Support `--format plain` and `--format json`.
- The CLI and MCP guidance must come from one shared implementation, not two
  independent copies.
- Guidance remains read-only and must not write drawers, runtime adoption
  events, knowledge cards, or lifecycle events.
- Guidance includes version, recording rule, required fields, optional fields,
  signal semantics, and track semantics.

## Boundaries

### Allowed Changes
- `src/core/**`
- `src/main.rs`
- `src/mcp/**`
- `tests/phase3_runtime.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-12-p62-runtime-adoption-cli-guidance.md`
- `specs/p62-runtime-adoption-cli-guidance.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change Phase-3 gate thresholds.
- Do not make card context default.
- Do not add card embeddings.
- Do not let CLI guidance mutate lifecycle state.

## Acceptance Criteria

Scenario: CLI guidance returns JSON without side effects
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_json_is_read_only
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption guidance --format json`
  Then stdout is valid JSON
  And the response includes `version=1`
  And the response states that only concrete runtime outcomes should be recorded
  And required fields include `track`, `signal`, and `feature`
  And signal guidance includes `used` and `rollback`
  And track guidance includes `card_context` with `include_cards`
  And runtime adoption event count remains zero

Scenario: CLI guidance supports plain output
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_plain
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption guidance`
  Then stdout mentions `version=1`
  And stdout mentions `recording_rule=record only concrete runtime outcomes, not speculation`
  And stdout mentions `signal=used`
  And stdout mentions `track=card_context`

Scenario: CLI guidance rejects unsupported format
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_rejects_invalid_format
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption guidance --format yaml`
  Then the command fails
  And stderr mentions `unsupported phase3 adoption format`

## Out of Scope

- MCP behavior changes beyond reusing the shared guidance implementation.
- Adoption-event deduplication.
- New analytics beyond existing `stats`.
- Any default-on policy changes.
