spec: task
name: "P98: release install doctor"
inherits: project
tags: [doctor, release, install, schema, phase-5]
---

## Intent

P98 adds an operator-facing release/install diagnostic surface. The immediate
problem is that a user or agent can run an older `mempal` binary against a newer
schema v9 database and see confusing failures. The doctor should explain binary,
PATH, and database schema compatibility without migrating or mutating state.

## Decisions

- Add CLI `mempal doctor --format plain|json`.
- Doctor runs before normal `Database::open` so it can report DB compatibility
  instead of failing on schema mismatch.
- Doctor reads SQLite `PRAGMA user_version` directly when `palace.db` exists.
- Doctor reports current binary version, supported schema version, configured
  DB path, DB schema version, current executable path, first `mempal` on PATH,
  warnings, and recommendations.
- Doctor is read-only and never creates `palace.db` or config files.

## Boundaries

### Allowed Changes
- specs/p98-release-install-doctor.spec.md
- docs/plans/2026-05-26-p98-release-install-doctor.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/doctor.rs
- src/lib.rs
- src/main.rs
- tests/ops_runtime.rs

### Forbidden
- Do not run `cargo install` from doctor.
- Do not migrate or create the database from doctor.
- Do not require network access.
- Do not remove support for existing `status`.

## Acceptance Criteria

Scenario: CLI doctor reports schema and install path diagnostics
  Test:
    Filter: cargo test --test ops_runtime test_cli_doctor_json_reports_schema_and_path
    Targets: `src/main.rs` `mempal doctor`.
  Given a temp HOME with an existing schema v9 `palace.db`
  And PATH resolves `mempal` to a different executable path
  When `mempal doctor --format json` runs
  Then JSON includes `current_version`
  And JSON includes `supported_schema_version=9`
  And JSON includes `db_schema_version=9`
  And JSON includes a warning containing `PATH`

Scenario: CLI doctor is read-only without an existing database
  Test:
    Filter: cargo test --test ops_runtime test_cli_doctor_plain_no_db_is_read_only
    Targets: read-only missing-db behavior.
  Given a temp HOME without `.mempal/palace.db`
  When `mempal doctor --format plain` runs
  Then the command succeeds
  And stdout reports `db_exists=false`
  And no `palace.db` file is created

Scenario: CLI doctor rejects unsupported format without side effects
  Test:
    Filter: cargo test --test ops_runtime test_cli_doctor_rejects_invalid_format
    Targets: format validation.
  Given a temp HOME without `.mempal/palace.db`
  When `mempal doctor --format yaml` runs
  Then the command fails
  And stderr mentions `unsupported doctor format`
  And no `palace.db` file is created

## Out of Scope

- Self-upgrade or install execution.
- GitHub release lookup.
- MCP runtime diagnostics; P99 covers the MCP surface.
