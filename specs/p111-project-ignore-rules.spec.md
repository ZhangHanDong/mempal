spec: task
name: "P111: project ignore rules for init and ingest"
inherits: project
tags: [cli, init, ingest, ignore, taxonomy, ux]
estimate: 1d
---

## Intent

P111 fixes a user-visible initialization problem: `mempal init` currently scans
generated/build directories into taxonomy rooms, and users have no project-level
way to exclude them. Add one shared project traversal filter for `mempal init`
room detection and directory-mode `mempal ingest`, so both entry points respect
the target project's ignore rules and mempal-specific ignore configuration.

## Decisions

- Add one shared project path-filter abstraction used by both `mempal init` and
  directory-mode `mempal ingest`; do not keep separate hard-coded traversal
  rules that can diverge.
- Default traversal respects the target project's Git ignore family
  (`.gitignore`, `.git/info/exclude`, and global Git excludes when available)
  and a mempal-specific `.mempalignore` file at the traversal root.
- Add the same CLI flags to `init` and `ingest`: repeatable
  `--ignore-file <path>`, `--no-gitignore`, and `--no-mempalignore`.
- Keep the existing hard traversal skips for backward-compatible safety even
  when ignore flags are disabled.
- Ignore rules apply to directory traversal and `init` room scanning. Explicit
  file ingest remains explicit: `mempal ingest path/to/file --wing ...` reads
  that file even if a directory traversal from the project root would ignore it.
- Use the mature `ignore` crate for Git-ignore-compatible matching instead of
  hand-writing `.gitignore` parsing.
- Do not change schema version, drawer IDs, embeddings, search ranking, or MCP
  tool contracts in P111.

## Boundaries

### Allowed Changes
- Cargo.toml
- Cargo.lock
- src/lib.rs
- src/path_filter.rs
- src/main.rs
- src/ingest/mod.rs
- tests/ingest_lock.rs
- README.md
- README_zh.md
- docs/usage.md
- specs/p111-project-ignore-rules.spec.md
- docs/plans/2026-07-06-p111-project-ignore-rules.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add a schema migration, database table, or database column.
- Do not rewrite, delete, or re-embed existing drawers.
- Do not change taxonomy routing scoring, search ranking, neighbor lookup, or
  MCP search/result response shapes.
- Do not make hidden files/directories globally ignored; only ignore them when
  matched by Git/mempal/custom ignore rules or hard skips.
- Do not make `.mempalignore` writeable or auto-generated in P111.

## Acceptance Criteria

Rule: init-project-ignore  Init room detection uses project ignore filtering

Scenario: init respects default project ignore rules
  Test:
    Filter: cargo test --bin mempal test_init_detect_rooms_respects_gitignore_and_mempalignore
  Given a project containing source room directories and generated directories
  And `.gitignore` ignores `dist/`
  And `.mempalignore` ignores `generated/`
  When room detection runs with default options
  Then rooms under source directories are detected
  And rooms under `dist/`, `generated/`, `.git/`, `target/`, and `node_modules/` are not detected

Scenario: init can disable Git ignore while keeping mempal ignore
  Test:
    Filter: cargo test --bin mempal test_init_no_gitignore_keeps_mempalignore
  Given `.gitignore` ignores `dist/`
  And `.mempalignore` ignores `generated/`
  When `mempal init <dir> --dry-run --no-gitignore` runs
  Then rooms under `dist/` are detected
  And rooms under `generated/` are not detected
  And hard-skipped rooms under `target/` are not detected

Scenario: init can use an explicit custom ignore file
  Test:
    Filter: cargo test --bin mempal test_init_custom_ignore_file_excludes_rooms
  Given a project containing `notes/api/`
  And a custom ignore file containing `notes/`
  When `mempal init <dir> --dry-run --ignore-file <custom>` runs
  Then room `api` from `notes/api/` is not printed or inserted

Scenario: missing custom ignore file fails clearly
  Test:
    Filter: cargo test --bin mempal test_init_missing_ignore_file_fails
  Level: integration
  Given no file exists at `<missing-ignore>`
  When `mempal init <dir> --ignore-file <missing-ignore>` runs
  Then the command exits with failure
  And stderr mentions the missing ignore file path
  And no taxonomy entry is inserted

Rule: ingest-project-ignore  Directory ingest uses project ignore filtering

Scenario: directory ingest respects default project ignore rules
  Test:
    Filter: cargo test --test ingest_lock test_ingest_dir_respects_gitignore_and_mempalignore
  Given a project with `keep.md`, `dist/ignored.md`, `generated/ignored.md`,
  and `target/ignored.md`
  And `.gitignore` ignores `dist/`
  And `.mempalignore` ignores `generated/`
  When directory ingest runs with default options
  Then only `keep.md` contributes chunks
  And no drawer source_file starts with `dist/`, `generated/`, or `target/`

Scenario: directory ingest can disable mempal ignore while keeping Git ignore
  Test:
    Filter: cargo test --test ingest_lock test_ingest_no_mempalignore_keeps_gitignore
  Given `.gitignore` ignores `dist/`
  And `.mempalignore` ignores `generated/`
  When directory ingest runs with `--no-mempalignore`
  Then `generated/remember.md` contributes chunks
  And `dist/ignored.md` does not contribute chunks

Scenario: directory ingest uses an explicit custom ignore file
  Test:
    Filter: cargo test --test ingest_lock test_ingest_custom_ignore_file_excludes_sources
  Given a project with `notes/ignored.md` and `keep.md`
  And a custom ignore file containing `notes/`
  When directory ingest runs with `--ignore-file <custom>`
  Then `keep.md` contributes chunks
  And no drawer source_file starts with `notes/`

Scenario: explicit file ingest bypasses traversal ignore rules
  Test:
    Filter: cargo test --test ingest_lock test_ingest_explicit_file_bypasses_project_ignore
  Given `.gitignore` ignores `dist/`
  And `dist/manual.md` exists
  When `mempal ingest dist/manual.md --wing test --room docs` runs
  Then `dist/manual.md` is ingested
  And exactly one drawer is written for that explicit file

## Out of Scope

- Retroactively deleting taxonomy entries or drawers polluted by earlier init or
  ingest runs.
- Adding an interactive cleanup command for existing rooms.
- Adding MCP or REST ignore configuration surfaces.
- Adding JSON output modes for `init` or `ingest`.
- Persisting ignore settings in `palace.db` or global mempal config.
