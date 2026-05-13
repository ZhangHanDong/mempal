spec: task
name: "P64: runtime adoption record quality policy"
inherits: project
tags: ["phase-3", "runtime-adoption", "quality", "cli", "mcp"]
---

## Intent

P63 can prepare exact runtime adoption record inputs, but it does not tell
agents whether the proposed event is high-quality enough to preserve as Phase-3
evidence. P64 adds a read-only quality policy that evaluates candidate adoption
record fields before writing, so agents can avoid low-signal evidence without
adding automatic recording or new runtime authority.

## Decisions

- Add CLI `mempal phase3 adoption check-record`.
- Add MCP `mempal_phase3 action=check_record`.
- The quality policy reuses the same `track`, `signal`, `feature`, and optional
  metadata parsing rules as `prepare-record` and `record`.
- The response returns `writes=false`, `valid`, `quality`, `errors`, and
  `warnings`.
- Empty `feature` is an error.
- `accepted`, `rejected`, `miss`, `rollback`, and `contradiction` should include
  a concrete `note` or `query`; missing both is a warning.
- Track-specific references are warnings, not hard errors:
  `card_context`/`card_embedding` prefer `card_id`, `evaluator` prefers
  `evaluator_id`, and `research_adapter` prefers `research_report_id`.
- The check remains advisory and must not append runtime adoption events.

## Boundaries

### Allowed Changes
- `src/core/phase3.rs`
- `src/main.rs`
- `src/mcp/**`
- `src/core/protocol.rs`
- `tests/phase3_runtime.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-13-p64-runtime-adoption-record-quality-policy.md`
- `specs/p64-runtime-adoption-record-quality-policy.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change Phase-3 gate thresholds.
- Do not make card context default.
- Do not add card embeddings.
- Do not block `record`; this policy is a preflight/advisory check only.

## Acceptance Criteria

Scenario: CLI check-record accepts a well-supported event
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_json_accepts_supported_event
  Level: integration
  Given an empty CLI HOME
  And `skill trigger` is the query text
  And `card evidence helped` is the note text
  When running `mempal phase3 adoption check-record --track card_context --signal accepted --feature include_cards --query "skill trigger" --card-id card_1 --note "card evidence helped" --format json`
  Then stdout is valid JSON
  And the response includes `writes=false`
  And `valid` is true
  And `quality` is `ready`
  And `errors` is empty
  And runtime adoption event count remains zero

Scenario: CLI check-record warns on weak evidence
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_json_warns_on_weak_evidence
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption check-record --track card_context --signal accepted --feature include_cards --format json`
  Then stdout is valid JSON
  And `valid` is true
  And `quality` is `warning`
  And `warnings` mentions missing concrete outcome context
  And `warnings` mentions missing `card_id`
  And runtime adoption event count remains zero

Scenario: CLI check-record rejects empty feature
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_rejects_empty_feature
  Level: integration
  Given an empty CLI HOME
  And the feature text contains only spaces
  When running `mempal phase3 adoption check-record --track card_context --signal accepted --feature "   " --format json`
  Then the command succeeds with an advisory JSON response
  And `valid` is false
  And `quality` is `invalid`
  And `errors` mentions `feature must not be empty`
  And runtime adoption event count remains zero

Scenario: MCP check_record is read-only
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_check_record_action_is_read_only
  Level: unit
  Given an empty test database
  When `mempal_phase3` is called with `action=check_record`
  Then the response includes `writes=false`
  And the response includes a quality report
  And runtime adoption event count remains zero

## Out of Scope

- Automatic recording after quality checks.
- Deduplicating or modifying existing adoption events.
- New readiness thresholds for Phase-3 gates.
- Default-on runtime policy changes.
