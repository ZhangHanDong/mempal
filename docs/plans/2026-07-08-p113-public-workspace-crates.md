# P113 Public Multi-Crate Workspace Split Plan

## Context

The repository currently publishes one package named `mempal`, with all modules
under `src/`. That shape was chosen after an earlier 8-crate workspace because
manual crates.io publication sequencing was fragile. P113 keeps the public
multi-crate goal, but treats publication order as a release-engineering concern:
the source tree becomes a Cargo workspace, each reusable boundary is a public
package, and the `mempal` package keeps the CLI facade.

The first split should avoid moving the SQLite database layer, MCP server
orchestration, and CLI command tree. Those are high-coupling integration
surfaces. Instead, extract reusable leaf or near-leaf boundaries and re-export
them through existing `mempal::*` paths.

## Implementation Steps

1. Keep the contract and inventory current.
   - Add `specs/p113-public-workspace-crates.spec.md`.
   - Add this plan.
   - Update `AGENTS.md` and `CLAUDE.md` to list P113 and the new workspace
     architecture after implementation.

2. Add RED tests first.
   - Add `tests/workspace_crates.rs`.
   - Assert the workspace members and root `path + version` dependencies.
   - Import `mempal_embed`, `mempal_search_core`,
     `mempal_agent_memory`, and `mempal_mcp_protocol` directly.
   - Assert legacy facade paths still compile through `mempal::embed`,
     `mempal::core::types`, `mempal::core::anchor`, and
     `mempal::core::protocol`.
   - Run the targeted test and confirm it fails before crates are added.

3. Convert the root manifest into a package + workspace manifest.
   - Keep `[package] name = "mempal"` at the root.
   - Add `[workspace]` with the root package and four reusable crates.
   - Add path+version dependencies from `mempal` to each reusable crate.
   - Forward root features to `mempal-embed`.

4. Extract `mempal-agent-memory`.
   - Move reusable memory domain types and anchor helpers into
     `crates/mempal-agent-memory`.
   - Re-export them from `src/core/mod.rs` so legacy paths remain stable.
   - Keep SQLite `Database`, config, phase3 persistence, and utils in the root
     package for P113.

5. Extract `mempal-embed`.
   - Move `Embedder`, `EmbedderFactory`, `EmbedError`, `ApiEmbedder`,
     `Model2VecEmbedder`, and `OnnxEmbedder` into `crates/mempal-embed`.
   - Keep `ConfiguredEmbedderFactory` in the root package because it depends on
     `mempal::core::config::Config`.
   - Re-export embed APIs through `mempal::embed`.

6. Extract `mempal-search-core`.
   - Add reusable RRF rank fusion and FTS5 query escaping helpers.
   - Update the root search module and DB helper search path to use the shared
     helpers.
   - Keep database-backed search execution in the root package.

7. Extract `mempal-mcp-protocol`.
   - Move `MEMORY_PROTOCOL` ownership into the new crate.
   - Re-export it through `mempal::core::protocol`.
   - Keep MCP server DTOs and tool handling in the root package for P113.

8. Update docs and inventories.
   - Update README architecture notes if they still describe single-crate
     internals.
   - Update `AGENTS.md` and `CLAUDE.md` from "single crate, no workspace" to
     the new workspace structure.

9. Verify.
   - `agent-spec parse specs/p113-public-workspace-crates.spec.md`
   - `agent-spec lint specs/p113-public-workspace-crates.spec.md --min-score 0.7`
   - `cargo test --test workspace_crates`
   - `cargo check --workspace`
   - `cargo test --workspace --all-features`
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo fmt -- --check`

## Non-Goals

- Do not release, tag, or publish crates in P113.
- Do not add `mempal-store-sqlite`; that should be a later P after the current
  leaf-crate split is stable.
- Do not change search ranking semantics, database schema, CLI command names,
  or MCP tool names.
