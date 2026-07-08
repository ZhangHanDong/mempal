spec: task
name: "P113: public multi-crate workspace split"
inherits: project
tags: [architecture, workspace, crates-io, rust, packaging]
estimate: 2d
---

## Intent

Split mempal from a single publishable package into a public multi-crate Cargo
workspace while preserving the existing `mempal` CLI package and command
surface. The reusable boundaries are embedder abstraction, hybrid-search core
primitives, agent-memory domain types, and MCP protocol text so downstream Rust
projects can depend on only the crates they need.

## Decisions

- Keep the top-level package name `mempal`; it remains the binary package used
  by `cargo install mempal`.
- Add public workspace member crates named `mempal-embed`,
  `mempal-search-core`, `mempal-agent-memory`, and `mempal-mcp-protocol`.
- The top-level `mempal` package depends on workspace member crates with both
  `path` and matching `version` requirements so local development uses the
  workspace and crates.io publication has a versioned dependency graph.
- `mempal-embed` owns the reusable `Embedder`, `EmbedderFactory`, `EmbedError`,
  API embedder, and feature-gated local embedder implementations.
- `mempal-search-core` owns reusable hybrid-search primitives such as FTS5
  query escaping and RRF rank fusion. It must not depend on mempal database,
  wing/room routing, tunnels, MCP, or agent-memory lifecycle services.
- `mempal-agent-memory` owns reusable memory domain types and anchor helpers
  that are not tied to CLI or MCP entry points.
- `mempal-mcp-protocol` owns the self-describing `MEMORY_PROTOCOL` text; the
  `mempal` package re-exports it through the legacy `mempal::core::protocol`
  path.
- Preserve legacy facade paths including `mempal::embed`,
  `mempal::core::types`, `mempal::core::anchor`, and
  `mempal::core::protocol` for existing tests and downstream users.
- Do not add a schema migration or change persisted SQLite table layouts in
  P113.

## Boundaries

### Allowed Changes
- Cargo.toml
- Cargo.lock
- src/**
- crates/mempal-embed/**
- crates/mempal-search-core/**
- crates/mempal-agent-memory/**
- crates/mempal-mcp-protocol/**
- tests/**
- README.md
- README_zh.md
- docs/usage.md
- specs/p113-public-workspace-crates.spec.md
- docs/plans/2026-07-08-p113-public-workspace-crates.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not rename the `mempal` binary or remove `cargo install mempal` support.
- Do not make the new reusable crates private or `publish = false`.
- Do not move database schema ownership into the new crates in P113.
- Do not change MCP tool names, CLI command names, JSON response shapes, or
  SQLite schema versions.
- Do not introduce a new runtime service, background daemon, network
  dependency, or alternate storage backend.

## Acceptance Criteria

Rule: public-workspace-crates  mempal is a public multi-crate workspace

Scenario: workspace metadata exposes the reusable public crates
  Test:
    Filter: cargo test --test workspace_crates test_workspace_manifest_lists_public_path_version_crates
  Level: integration
  Targets: Cargo workspace manifest and package manifests
  Given the root `Cargo.toml`
  When it is parsed as Cargo metadata
  Then the workspace members include `mempal-embed`
  And the workspace members include `mempal-search-core`
  And the workspace members include `mempal-agent-memory`
  And the workspace members include `mempal-mcp-protocol`
  And the top-level `mempal` dependencies use both local `path` and matching `version` requirements for those crates
  And the reusable crates are publishable public packages

Scenario: reusable crates compile and expose their intended public APIs
  Test:
    Filter: cargo test --test workspace_crates test_public_reusable_crates_are_directly_usable
  Level: integration
  Targets: direct public crate imports
  Given an integration test that imports the reusable crates directly
  When the test uses `mempal-embed`, `mempal-search-core`, `mempal-agent-memory`, and `mempal-mcp-protocol`
  Then the imports compile without going through the top-level `mempal` facade
  And `mempal-search-core` returns deterministic RRF and FTS5 query results
  And `mempal-mcp-protocol` returns `MEMORY_PROTOCOL`

Scenario: legacy mempal facade paths remain compatible
  Test:
    Filter: cargo test --test workspace_crates test_mempal_facade_preserves_legacy_public_paths
  Level: integration
  Targets: legacy public facade paths
  Given existing downstream code using `mempal::embed`
  And existing downstream code using `mempal::core::types`
  And existing downstream code using `mempal::core::anchor`
  And existing downstream code using `mempal::core::protocol`
  When the integration test compiles
  Then those legacy paths still resolve to the extracted crate APIs

Scenario: mempal CLI package remains the installable package
  Test:
    Filter: cargo metadata --no-deps --format-version 1
  Level: metadata
  Targets: Cargo package targets
  Given the multi-crate workspace
  When Cargo metadata is requested
  Then the package named `mempal` still has a binary target named `mempal`
  And the package named `mempal` remains publishable
  And the root package version matches the reusable crate dependency versions

Scenario: workspace builds without default-feature regressions
  Test:
    Filter: cargo check --workspace
  Level: compile
  Targets: workspace default features
  Given the multi-crate workspace
  When the default feature build is checked
  Then every workspace member compiles
  And the top-level default `model2vec` feature still reaches the embed crate

Scenario: workspace all-features build keeps optional surfaces wired
  Test:
    Filter: cargo test --workspace --all-features
  Level: compile
  Targets: workspace all-features build
  Given the multi-crate workspace
  When all features are enabled
  Then the REST, ONNX, model2vec, MCP, search, ingest, and memory tests compile
  And no public API path required by existing tests is removed

Scenario: private crate markers and schema migrations are rejected by policy
  Test:
    Filter: cargo test --test workspace_crates test_workspace_split_does_not_mark_reusable_crates_private_or_change_schema
  Level: integration
  Targets: package manifests and schema constant
  Given the reusable crate manifests
  When they are inspected for publication metadata
  Then none of the reusable crates contains `publish = false`
  And the root `CURRENT_SCHEMA_VERSION` remains "9"
  And no new migration file or schema version is required for P113

## Out of Scope

- Automating the multi-crate crates.io publish command sequence.
- Releasing or tagging a new version.
- Moving SQLite `Database` and schema migrations into `mempal-store-sqlite`.
- Fully extracting MCP server orchestration from the `mempal` package.
- Changing runtime behavior of search ranking, context assembly, ingest, or MCP
  tools beyond using the newly extracted crates.
