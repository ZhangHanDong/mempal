spec: task
name: "P109: cross-project resume"
inherits: project
tags: [projects, resume, cli, mcp, cross-project]
estimate: 1d
---

## Intent

P109 lets an agent or operator, from ANY directory, list every project mempal
knows and resume one by fuzzy name — without already knowing its wing slug or
absolute path. mempal already stores every project's memory in one global
`palace.db` keyed by `wing`, and each project's absolute worktree path lives in
its `worktree://` anchor_id; P109 surfaces that as two read-only entrypoints so
"continue project xxx" becomes a single resolve→load step. The resolved pack
includes the project's path (to cd into), recent decisions, in-flight candidate
knowledge, and a concrete next step. mempal supplies the memory and the path;
actually moving into the repo stays the agent's job.

## Decisions

- New library module `src/projects.rs` with deterministic, embedder-free,
  read-only functions over `Database`:
  - `list_projects` returns one `ProjectSummary` per `wing` (path from the
    latest `worktree://` anchor, total/evidence/knowledge counts, last activity),
    ordered by most-recent activity then wing.
  - `resume_project(query, evidence_limit, candidate_limit)` resolves a fuzzy
    `query` against wing names and worktree-path basenames (case-insensitive),
    with an exact wing match taking precedence, returning a `ResumeResolution`:
    `Resolved` (one match → a `ResumePack` with path, counts, recent evidence,
    in-flight candidate knowledge, and a `next_step` string), `Ambiguous` (>1
    match → the candidate summaries), or `NotFound` (0 → the available wings).
- Both functions are read-only: no LLM, no embeddings, no database writes.
- CLI: `mempal projects [--format plain|json]` and `mempal resume <query>
  [--evidence-limit N] [--candidate-limit N] [--format plain|json]`.
- MCP: `mempal_projects` (no args) and `mempal_resume` (`query` plus optional
  `evidence_limit` / `candidate_limit`), returning the same data.
- Path resolution relies on `worktree://` anchors; a project whose drawers only
  carry legacy `repo://legacy` anchors resolves with `path = null` and a
  next_step that says the worktree path is unknown. This is surfaced, not hidden.

## Boundaries

### Allowed Changes
- src/projects.rs
- src/lib.rs
- src/core/db.rs
- src/main.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- src/core/protocol.rs
- specs/p109-cross-project-resume.spec.md
- docs/plans/2026-06-22-p109-cross-project-resume.md
- CHANGELOG.md
- Cargo.toml
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not write to the database, embed, or call an LLM in projects/resume.
- Do not change `mempal context` / `mempal_context` assembly or its defaults.
- Do not auto-`cd`, check out, or otherwise mutate any working tree.
- Do not add new schema tables, columns, schema versions, or feature flags.

## Acceptance Criteria

Scenario: list_projects reports each wing with its worktree path and counts
  Test:
    Filter: cargo test --test projects_resume test_list_projects_reports_paths_and_counts
  Given drawers in two wings each with a worktree anchor
  When projects are listed
  Then both wings are returned
  And each carries its absolute worktree path and drawer counts

Scenario: resume resolves a unique fuzzy match to a pack with path and evidence
  Test:
    Filter: cargo test --test projects_resume test_resume_resolves_unique_match
  Given one wing whose name contains the query and recent evidence drawers
  When the project is resumed
  Then the resolution is resolved
  And the pack carries that wing's worktree path
  And the pack lists recent evidence

Scenario: resume reports ambiguity without guessing
  Test:
    Filter: cargo test --test projects_resume test_resume_reports_ambiguous_matches
  Given two wings whose names both contain the query
  When the project is resumed
  Then the resolution is ambiguous
  And both wings are offered as candidates

Scenario: resume reports not-found with the available projects
  Test:
    Filter: cargo test --test projects_resume test_resume_reports_not_found
  Given a query matching no wing or path
  When the project is resumed
  Then the resolution is not found
  And the available wings are listed

Scenario: CLI exposes projects and resume commands
  Test:
    Filter: cargo test --bin mempal test_cli_projects_and_resume_parse
  Given the argument vectors for `mempal projects` and `mempal resume <query>`
  When the CLI parses them
  Then each maps to its command variant carrying the expected fields

Scenario: MCP registers the projects and resume tools
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_registry_includes_projects_and_resume
  Given the MCP tool registry
  When the tool list is read
  Then it contains `mempal_projects` and `mempal_resume`

## Out of Scope

- Embedding- or LLM-based ranking of projects or evidence (count/recency only).
- Auto-`cd`, repo checkout, or launching the project; resume returns guidance.
- A persistent project registry table; data is derived live from drawers.
- Backfilling worktree paths for legacy `repo://legacy`-only projects.
- mtime-based active-session selection (separate future work).
