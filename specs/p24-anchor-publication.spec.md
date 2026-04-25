spec: task
name: "P24: explicit knowledge anchor publication CLI"
inherits: project
tags: [memory, knowledge, anchor, lifecycle, cli]
---

## Intent

P12 introduced `global/repo/worktree` anchors and P14-P23 made typed knowledge usable at runtime, but anchor-scope publication still exists only as a design rule. P24 adds the smallest explicit CLI operation for publishing an existing knowledge drawer outward across anchor scopes while preserving Stage-1 drawer storage and avoiding automatic policy decisions.

## Decisions

- Add CLI command `mempal knowledge publish-anchor <drawer_id>`.
- Request flags:
  - required `--to repo|global`
  - required `--reason <text>`
  - optional `--reviewer <text>`
  - optional `--target-anchor-id <anchor_id>`
  - optional `--cwd <path>`
- The command only accepts `memory_kind=knowledge`.
- The command only accepts active knowledge statuses: `promoted|canonical`.
- The command is metadata-only:
  - updates `anchor_kind`, `anchor_id`, and `parent_anchor_id`
  - does not change content, statement, tier, status, refs, vectors, FTS content, source_file, or schema
- Publication chain is explicit and outward-only:
  - `worktree -> repo`
  - `repo -> global`
  - `worktree -> global` is rejected
  - same-anchor publication is rejected
  - inward publication is rejected
- For `--to repo`:
  - if `--target-anchor-id` is supplied, it must be a valid `repo://...` anchor id
  - otherwise, a worktree drawer must have `parent_anchor_id`
  - otherwise, `--cwd` may be used to derive the current repo anchor
- For `--to global`:
  - `--target-anchor-id` is required and must be a valid `global://...` anchor id
  - the drawer must already have `domain=global`, preserving the P12 invariant that global anchors require global domain
- Successful publication appends a `knowledge_publish_anchor` audit entry with drawer id, old/new anchor, reason, and reviewer.
- The command prints a deterministic one-line summary: `published <drawer_id>: <old_kind>:<old_id> -> <new_kind>:<new_id>`.
- This task does not add MCP or REST publication endpoints.

## Boundaries

### Allowed Changes

- `src/core/db.rs`
- `src/knowledge_anchor.rs`
- `src/lib.rs`
- `src/main.rs`
- `tests/knowledge_lifecycle.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p24-anchor-publication.spec.md`
- `docs/plans/**`

### Forbidden

- Do not add tables, columns, triggers, or migrations.
- Do not add Phase-2 `knowledge_cards`.
- Do not duplicate or clone drawers.
- Do not update vectors or re-embed content.
- Do not change `mempal_context` ordering.
- Do not add MCP or REST publication endpoints.
- Do not auto-publish as part of promote/distill/gate.
- Do not introduce new dependencies.

## Acceptance Criteria

Scenario: CLI publishes worktree knowledge to parent repo anchor
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_worktree_to_repo
  Given a promoted knowledge drawer has `anchor_kind="worktree"` and `parent_anchor_id="repo://parent"`
  When running `mempal knowledge publish-anchor <id> --to repo --reason "share stable rule"`
  Then the command exits successfully
  And stdout says `published <id>: worktree:<old_id> -> repo:repo://parent`
  And the stored drawer has `anchor_kind="repo"`, `anchor_id="repo://parent"`, and `parent_anchor_id=NULL`
  And content, statement, status, refs, and vector row remain unchanged
  And one `knowledge_publish_anchor` audit entry is appended

Scenario: CLI publishes global-domain repo knowledge to explicit global anchor
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_repo_to_global
  Given a canonical knowledge drawer has `domain="global"` and `anchor_kind="repo"`
  When running `mempal knowledge publish-anchor <id> --to global --target-anchor-id global://epistemics --reason "global law" --reviewer human`
  Then the command exits successfully
  And the stored drawer has `anchor_kind="global"` and `anchor_id="global://epistemics"`
  And the audit entry includes reviewer `human`

Scenario: CLI rejects worktree directly to global
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_rejects_worktree_to_global
  Given a promoted knowledge drawer has `anchor_kind="worktree"`
  When running `mempal knowledge publish-anchor <id> --to global --target-anchor-id global://x --reason "skip"`
  Then the command fails
  And stderr mentions `worktree -> global publication is not allowed`
  And the stored drawer anchor remains unchanged

Scenario: CLI rejects inactive or evidence targets
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_rejects_inactive_or_evidence
  Given an evidence drawer exists
  When running `mempal knowledge publish-anchor <evidence_id> --to repo --reason "bad"`
  Then the command fails
  And stderr mentions `knowledge anchor publication requires a knowledge drawer`
  Given a candidate knowledge drawer exists
  When running `mempal knowledge publish-anchor <candidate_id> --to repo --reason "bad"`
  Then the command fails
  And stderr mentions `publish-anchor requires promoted or canonical knowledge`

Scenario: CLI rejects invalid target anchors
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_rejects_invalid_target_anchor
  Given a promoted repo knowledge drawer exists
  When running `mempal knowledge publish-anchor <id> --to global --reason "missing target"`
  Then the command fails
  And stderr mentions `--target-anchor-id is required for global publication`
  When running `mempal knowledge publish-anchor <id> --to repo --target-anchor-id global://wrong --reason "bad"`
  Then the command fails
  And stderr mentions `expected prefix repo://`

Scenario: publish-anchor does not bump schema
  Test:
    Package: mempal
    Filter: test_cli_knowledge_publish_anchor_does_not_bump_schema
  Given a promoted worktree knowledge drawer with a parent repo anchor
  And the current schema version is recorded
  When running `mempal knowledge publish-anchor <id> --to repo --reason "stable"`
  Then schema version remains unchanged
  And no new table is created

## Out of Scope

- MCP `mempal_knowledge_publish_anchor`.
- REST publication endpoint.
- Automatic publication after promotion.
- LLM or evaluator scoring.
- Phase-2 knowledge card publication.
