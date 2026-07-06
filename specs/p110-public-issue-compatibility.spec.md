spec: task
name: "P110: public issue compatibility fixes"
inherits: project
tags: [github-issues, ingest, mcp, windows, compatibility]
estimate: 1d
---

## Intent

P110 resolves the four currently open public GitHub issues for mempal:
drawer identity collisions on identical content from different sources, Windows
`~/` expansion, MCP JSON Schema integer-format warnings, and MCP search output
schema mismatches when `tunnel_hints` is empty. The fix keeps existing stored
drawers valid while making new writes and MCP responses stable across Windows
and stricter MCP clients such as opencode.

## Decisions

- New bootstrap drawer IDs use a 12-hex digest suffix for newly generated
  bootstrap IDs; legacy stored drawer IDs are not migrated or rewritten.
- Evidence drawer identity includes the explicit source identity when one is
  available. File ingest uses normalized `source_file`; REST/MCP manual ingest
  uses the provided `source` field when non-empty and otherwise preserves the
  existing same-content dedup behavior.
- File ingest skips `.DS_Store`, AppleDouble `._*` files, and common binary
  extensions before read/normalize/embed so non-text artifacts do not create
  empty or lossy evidence drawers.
- `Database::insert_drawer` returns whether a row was inserted. Callers insert
  vectors only when a drawer insert actually occurred, because sqlite-vec
  virtual tables must not receive duplicate vector inserts.
- CLI home expansion uses platform home discovery instead of only `HOME`, and
  handles both `~` and `~/...`.
- MCP `StatusResponse` schema avoids unsigned integer JSON Schema formats for
  fields exposed to clients that warn on `uint32` / `uint64`.
- MCP `SearchResultDto.tunnel_hints` is always serialized, even when empty, so
  the response shape matches the tool output schema.

## Boundaries

### Allowed Changes
- src/core/utils.rs
- src/core/db.rs
- src/ingest/mod.rs
- src/api/handlers.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- src/cowork/bus.rs
- src/main.rs
- tests/ingest_lock.rs
- tests/mind_model_bootstrap.rs
- tests/normalize_version.rs
- tests/projects_resume.rs
- specs/p110-public-issue-compatibility.spec.md
- docs/plans/2026-07-02-p110-public-issue-compatibility.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add a schema migration, new database table, or database column.
- Do not rewrite existing drawer IDs or vectors.
- Do not add new runtime dependencies.
- Do not remove existing MCP fields or rename public tool names.

## Acceptance Criteria

Scenario: file ingest distinguishes identical content from different sources
  Test:
    Filter: cargo test --test ingest_lock test_ingest_same_content_different_sources_get_distinct_drawers
  Given two different text files with identical normalized content
  When both files are ingested into the same wing and room
  Then both writes succeed
  And two active drawers exist
  And the drawer IDs are different

Scenario: same-source ingest remains idempotent
  Test:
    Filter: cargo test --test ingest_lock test_double_check_after_lock_skips_duplicate
  Given the same file is ingested twice through the locked path
  When the second ingest runs after the first committed
  Then it skips the existing drawer
  And it does not insert a duplicate vector

Scenario: directory ingest skips platform and binary artifacts
  Test:
    Filter: cargo test --test ingest_lock test_ingest_dir_skips_platform_and_binary_files
  Given a directory containing a text file, `.DS_Store`, an AppleDouble file,
  and a binary artifact
  When the directory is ingested
  Then only the text file contributes chunks
  And skipped artifact files are counted as skipped files

Scenario: CLI home expansion handles Windows-style missing HOME
  Test:
    Filter: cargo test --bin mempal test_expand_home_handles_tilde_without_home_env
  Given a path equal to `~` or starting with `~/`
  When home discovery is available without relying on `HOME`
  Then the returned path is not a literal `~` subdirectory

Scenario: MCP status schema uses signed integer-compatible formats
  Test:
    Filter: cargo test --lib mcp::tools::tests::test_status_response_schema_avoids_unsigned_integer_formats
  Given the generated MCP schema for `StatusResponse`
  When the schema JSON is inspected
  Then it does not contain `uint32`
  And it does not contain `uint64`

Scenario: MCP search results always serialize tunnel_hints
  Test:
    Filter: cargo test --lib mcp::tools::tests::test_search_result_serializes_empty_tunnel_hints
  Given a search result with no tunnel hints
  When it is converted to `SearchResultDto` and serialized
  Then the JSON object contains `tunnel_hints`
  And the value is an empty array

## Out of Scope

- Backfilling or re-keying existing drawers.
- Changing semantic dedup similarity behavior.
- Changing `mempal_search` ranking, routing, or neighbor semantics.
- Closing or commenting on GitHub issues from the agent session.
