# P110 Implementation Plan — public issue compatibility fixes

Spec: `specs/p110-public-issue-compatibility.spec.md`
Target release: patch release after v0.7.0

## Context

The public GitHub issue tracker currently reports four compatibility problems:

- #1: identical content from different source files can collide into the same
  drawer identity.
- #2: `~/` expansion uses `HOME`, which fails under Windows CMD and creates a
  literal `~` directory.
- #3: MCP JSON Schema exposes unsigned integer formats that opencode warns
  about.
- #4: MCP search results omit empty `tunnel_hints`, while the output schema
  requires the property.

These are small but user-visible compatibility fixes. They do not need a schema
migration; they should harden new writes and wire output shape while leaving
existing stored drawers intact.

## Tasks

- [x] T1. Spec and plan.
- [x] T2. Add failing regression tests:
      - identical content in different files produces distinct drawers;
      - same-source duplicate ingest remains idempotent;
      - directory ingest skips `.DS_Store`, `._*`, and binary artifacts;
      - `expand_home` handles `~` and `~/...` without relying on `HOME`;
      - `StatusResponse` schema has no `uint32` / `uint64`;
      - empty `tunnel_hints` is serialized as `[]`.
- [x] T3. Drawer identity:
      - extend bootstrap ID suffix to 12 hex chars;
      - include source identity in evidence ID helpers when explicit source is
        available;
      - pass normalized `source_file` from file ingest and request `source`
        from REST/MCP manual ingest.
- [x] T4. Insert semantics:
      - make `Database::insert_drawer` return `bool`;
      - use `INSERT OR IGNORE`;
      - only insert vectors when a drawer row was actually inserted.
- [x] T5. File skipping:
      - add `should_skip_file`;
      - skip platform metadata and common binary extensions before reading.
- [x] T6. Windows home expansion:
      - implement platform home fallback using `HOME`, `USERPROFILE`,
        `HOMEDRIVE` + `HOMEPATH`, then current directory fallback;
      - handle exact `~`.
- [x] T7. MCP wire compatibility:
      - remove `skip_serializing_if` from `SearchResultDto.tunnel_hints`;
      - expose status schema integer fields as signed-compatible types.
- [x] T8. Sync P110 inventory in `AGENTS.md` and `CLAUDE.md`.
- [x] T9. Verify:
      - `agent-spec parse/lint` for P110;
      - all P110 scenario filters;
      - `cargo fmt --check`;
      - `cargo check`;
      - `cargo clippy -- -D warnings`;
      - `cargo test`.

## Rollback

Revert the P110 commit. Existing databases need no rollback because P110 does
not migrate or rewrite stored rows; reverting only restores older ID generation
and MCP serialization behavior for future writes/responses.
