spec: task
name: "P114 Coarse Workspace Split"
inherits: project
tags: [architecture, workspace, crates, rust]
---

## Intent

Split the remaining root-heavy implementation into a small number of public,
publishable workspace crates so downstream Rust projects can reuse mempal
storage, runtime, and MCP server layers without depending on the CLI package as
their implementation boundary. The root `mempal` package must remain the
installable CLI crate and must preserve legacy public facade paths for existing
users.

## Decisions

- Add exactly three coarse implementation crates: `mempal-store-sqlite`, `mempal-runtime`, and `mempal-mcp-server`.
- Keep the existing P113 crates: `mempal-agent-memory`, `mempal-embed`, `mempal-search-core`, and `mempal-mcp-protocol`.
- Keep the root package name `mempal`; `cargo install mempal` semantics must not move to a different package.
- Move SQLite ownership into `mempal-store-sqlite`: `Database`, `DbError`, migrations, `CURRENT_SCHEMA_VERSION`, FTS5, sqlite-vec, knowledge-card tables, runtime-adoption tables, KG triples, and tunnels.
- Move business/runtime modules into `mempal-runtime`: ingest, search orchestration, context, brief, knowledge lifecycle/gate/distill/card helpers, fact checking, projects/resume, cowork runtime, doctor reports, field taxonomy, adoption analytics, and AAAK helpers.
- Move MCP server implementation into `mempal-mcp-server`: rmcp server type and tool request/response wiring.
- Preserve legacy root facade paths including `mempal::core::db`, `mempal::search`, `mempal::ingest`, `mempal::context`, `mempal::mcp::MempalMcpServer`, and existing P113 facades.
- Do not add fine-grained crates for individual features such as knowledge, cowork, factcheck, or context in P114.
- Do not change SQLite schema version in P114.

## Boundaries

### Allowed Changes
- **/Cargo.toml
- **/Cargo.lock
- **/crates/**
- **/src/**
- **/tests/**
- **/specs/p114-coarse-workspace-split.spec.md
- **/docs/plans/2026-07-09-p114-coarse-workspace-split.md
- **/README.md
- **/README_zh.md
- **/docs/usage.md
- **/AGENTS.md
- **/CLAUDE.md

### Forbidden
- Do not rename the root package away from `mempal`.
- Do not mark reusable workspace crates with `publish = false`.
- Do not add feature-level crates such as `mempal-knowledge`, `mempal-cowork`, `mempal-context`, or `mempal-factcheck`.
- Do not bump `CURRENT_SCHEMA_VERSION` above `9`.
- Do not remove legacy facade modules from the root crate.

## Completion Criteria

Scenario: Workspace lists only the coarse new implementation crates
  Test: test_p114_workspace_lists_coarse_runtime_crates
  Level: integration
  Targets: Cargo workspace manifest and coarse crate boundary
  Given the root Cargo workspace manifest
  When the workspace members are inspected
  Then it contains `crates/mempal-store-sqlite`, `crates/mempal-runtime`, and `crates/mempal-mcp-server`
  And it still contains the existing P113 crates
  And it does not contain feature-level crates such as `mempal-knowledge`, `mempal-cowork`, `mempal-context`, or `mempal-factcheck`

Scenario: Store crate owns SQLite database surface
  Test: test_p114_store_sqlite_crate_owns_database_schema
  Level: integration
  Targets: direct public store crate imports
  Given the public `mempal-store-sqlite` crate
  When a downstream crate imports its database API
  Then `mempal_store_sqlite::Database`, `mempal_store_sqlite::DbError`, and `mempal_store_sqlite::CURRENT_SCHEMA_VERSION` are usable
  And `CURRENT_SCHEMA_VERSION` remains `9`

Scenario: Runtime and MCP crates are directly reusable
  Test: test_p114_runtime_and_mcp_crates_are_directly_usable
  Level: integration
  Targets: direct runtime and MCP crate imports
  Given the public runtime and MCP server crates
  When a downstream crate imports runtime and MCP APIs
  Then `mempal_runtime::search`, `mempal_runtime::ingest`, `mempal_runtime::context`, and `mempal_mcp_server::MempalMcpServer` are usable without importing root implementation modules

Scenario: Root facade preserves legacy public paths
  Test: test_p114_root_preserves_legacy_facades
  Level: integration
  Targets: legacy root facade imports
  Given code written against the pre-P114 root crate facade
  When it imports `mempal::core::db`, `mempal::search`, `mempal::ingest`, `mempal::context`, and `mempal::mcp::MempalMcpServer`
  Then those paths continue to compile and point at the split workspace crate implementations

Scenario: Publishability and installable root package are preserved
  Test: test_p114_publishable_crates_and_root_package_are_preserved
  Level: integration
  Targets: Cargo package manifests
  Given the workspace manifests
  When package metadata is inspected
  Then the root package remains named `mempal`
  And every reusable workspace crate remains publishable
  And root dependencies on workspace crates use path plus version entries

## Out of Scope

- Publishing the new member crates to crates.io in this PR.
- Changing the SQLite schema, migration version, or on-disk database format.
- Splitting REST API into a separate crate.
- Introducing new storage backends beyond SQLite.
