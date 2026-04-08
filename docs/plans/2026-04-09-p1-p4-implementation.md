# P1-P4 Implementation Plan: Routing → MCP → AAAK → REST

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the remaining documented phases after P0 so mempal supports routing-aware retrieval, MCP integration, AAAK output formatting, and a feature-gated REST API.

**Architecture:** Build the remaining phases in the documented order. P1 extends `mempal-search` and `mempal-cli` with deterministic taxonomy routing and richer CLI output. P2 adds a minimal MCP server crate and `serve --mcp`. P3 keeps AAAK isolated on the output path only. P4 adds a feature-gated axum REST server and reuses the same search/ingest/status logic.

**Tech Stack:** Rust 2024, rusqlite, sqlite-vec, clap 4, tokio, serde/serde_json, anyhow/thiserror, rmcp, nom, bimap, axum

**Specs:** `specs/p1-routing-citation.spec.md`, `specs/p2-mcp.spec.md`, `specs/p3-aaak.spec.md`, `specs/p4-rest-api.spec.md`

---

## Task 1: P1 Search Routing and Explainable Decisions

**Files:**
- Modify: `crates/mempal-core/src/types.rs`
- Modify: `crates/mempal-core/src/lib.rs`
- Modify: `crates/mempal-search/src/lib.rs`
- Create: `crates/mempal-search/src/route.rs`
- Test: `crates/mempal-search/tests/route_test.rs`
- Test: `crates/mempal-search/tests/search_test.rs`

- [ ] **Step 1:** Write failing routing tests for `route_query()` hit, fallback, and explainability.
- [ ] **Step 2:** Run `cargo test -p mempal-search route_ -- --nocapture` and verify the new tests fail for the expected missing symbols.
- [ ] **Step 3:** Add `RouteDecision` to core types and implement deterministic keyword scoring in `route.rs`.
- [ ] **Step 4:** Extend search results to carry route metadata and update `search()` to load taxonomy, route first, and apply routed wing/room unless confidence is below 0.5.
- [ ] **Step 5:** Run `cargo test -p mempal-search` and verify all search and routing tests pass.
- [ ] **Step 6:** Commit: `feat(search): add deterministic routing with explainable decisions`

## Task 2: P1 CLI Wake-Up, Taxonomy, and Richer Status

**Files:**
- Modify: `crates/mempal-cli/src/main.rs`
- Modify: `crates/mempal-cli/tests/cli_test.rs`
- Modify: `crates/mempal-core/src/db.rs`
- Modify: `crates/mempal-core/src/lib.rs`

- [ ] **Step 1:** Write failing CLI tests for `wake-up`, richer `status`, and `taxonomy list` / `taxonomy edit`.
- [ ] **Step 2:** Run `cargo test -p mempal-cli test_cli_ -- --nocapture` and verify the new tests fail with unknown subcommands or mismatched output.
- [ ] **Step 3:** Add helper queries in core for taxonomy listing/updating, top-drawer summaries, and DB file size.
- [ ] **Step 4:** Implement CLI subcommands `wake-up`, `taxonomy list`, `taxonomy edit`, and expand `status` output to include wing/room counts and database size.
- [ ] **Step 5:** Run `cargo test -p mempal-cli` and verify P0 + P1 CLI coverage passes.
- [ ] **Step 6:** Commit: `feat(cli): add wake-up and taxonomy commands`

## Task 3: P2 MCP Core Tools

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mempal-mcp/Cargo.toml`
- Create: `crates/mempal-mcp/src/server.rs`
- Create: `crates/mempal-mcp/src/tools.rs`
- Modify: `crates/mempal-mcp/src/lib.rs`
- Test: `crates/mempal-mcp/tests/mcp_test.rs`

- [ ] **Step 1:** Write failing MCP tests for tool listing plus `mempal_status`, `mempal_search`, and `mempal_ingest`.
- [ ] **Step 2:** Run `cargo test -p mempal-mcp` and verify the new tests fail because the server and tools are not implemented.
- [ ] **Step 3:** Add the `rmcp` dependency and implement a minimal stdio MCP server exposing four tools: status, search, ingest, taxonomy.
- [ ] **Step 4:** Reuse existing core/search/ingest helpers inside tool handlers and keep returned payloads JSON-serializable.
- [ ] **Step 5:** Run `cargo test -p mempal-mcp` and verify all MCP tests pass.
- [ ] **Step 6:** Commit: `feat(mcp): add stdio MCP server with four tools`

## Task 4: P2 CLI Serve Integration

**Files:**
- Modify: `crates/mempal-cli/src/main.rs`
- Modify: `crates/mempal-cli/Cargo.toml`
- Modify: `crates/mempal-cli/tests/cli_test.rs`

- [ ] **Step 1:** Write a failing CLI test that `mempal serve --mcp --help` exposes MCP serving mode and `serve` parses successfully.
- [ ] **Step 2:** Run `cargo test -p mempal-cli serve -- --nocapture` and verify the new test fails.
- [ ] **Step 3:** Add a `serve` subcommand and wire `--mcp` to the MCP server entry point.
- [ ] **Step 4:** Keep plain `serve` ready for later REST integration without changing the P2 behavior contract.
- [ ] **Step 5:** Run `cargo test -p mempal-cli` and verify serve parsing does not regress other commands.
- [ ] **Step 6:** Commit: `feat(cli): add serve command for MCP mode`

## Task 5: P3 AAAK Parse, Encode, Decode, and Roundtrip

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mempal-aaak/Cargo.toml`
- Modify: `crates/mempal-aaak/src/lib.rs`
- Create: `crates/mempal-aaak/src/model.rs`
- Create: `crates/mempal-aaak/src/parse.rs`
- Create: `crates/mempal-aaak/src/codec.rs`
- Test: `crates/mempal-aaak/tests/aaak_test.rs`

- [ ] **Step 1:** Write failing AAAK tests for parse, invalid parse, encode, decode, entity bimap, version marker, truncation report, and roundtrip coverage.
- [ ] **Step 2:** Run `cargo test -p mempal-aaak` and verify the new tests fail because the codec is still a stub.
- [ ] **Step 3:** Add `nom` and `bimap` dependencies, define document models, and implement the BNF parser.
- [ ] **Step 4:** Implement encoder, decoder, and `verify_roundtrip()` with transparent truncation and loss reporting.
- [ ] **Step 5:** Run `cargo test -p mempal-aaak` and verify the full codec suite passes.
- [ ] **Step 6:** Commit: `feat(aaak): implement codec with parser and roundtrip verification`

## Task 6: P3 CLI AAAK Output and Compression Commands

**Files:**
- Modify: `crates/mempal-cli/src/main.rs`
- Modify: `crates/mempal-cli/Cargo.toml`
- Modify: `crates/mempal-cli/tests/cli_test.rs`

- [ ] **Step 1:** Write failing CLI tests for `wake-up --format aaak` and `compress`.
- [ ] **Step 2:** Run `cargo test -p mempal-cli aaak -- --nocapture` and verify the new tests fail with unknown options or commands.
- [ ] **Step 3:** Wire `mempal-aaak` into CLI output only, formatting wake-up summaries as AAAK and adding a `compress` command for arbitrary input text.
- [ ] **Step 4:** Keep storage and search paths free of AAAK dependencies beyond CLI formatting.
- [ ] **Step 5:** Run `cargo test -p mempal-cli` and verify existing CLI flows still pass.
- [ ] **Step 6:** Commit: `feat(cli): add AAAK output formatting commands`

## Task 7: P4 Feature-Gated REST API Crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mempal-api/Cargo.toml`
- Modify: `crates/mempal-api/src/lib.rs`
- Create: `crates/mempal-api/src/handlers.rs`
- Create: `crates/mempal-api/src/state.rs`
- Test: `crates/mempal-api/tests/api_test.rs`

- [ ] **Step 1:** Write failing API tests for `/api/search`, `/api/ingest`, `/api/taxonomy`, `/api/status`, and the no-rest-feature build contract.
- [ ] **Step 2:** Run `cargo test -p mempal-api` and verify the new tests fail because handlers and routes do not exist.
- [ ] **Step 3:** Add optional axum dependencies, define shared app state, and implement the four JSON endpoints.
- [ ] **Step 4:** Keep the API crate reusable from CLI `serve` and ensure response shapes match the spec.
- [ ] **Step 5:** Run `cargo test -p mempal-api --features rest` and verify endpoint tests pass.
- [ ] **Step 6:** Commit: `feat(api): add feature-gated REST API`

## Task 8: P4 CLI Serve Integration for MCP + REST

**Files:**
- Modify: `crates/mempal-cli/Cargo.toml`
- Modify: `crates/mempal-cli/src/main.rs`
- Modify: `crates/mempal-cli/tests/cli_test.rs`

- [ ] **Step 1:** Write failing CLI tests for `mempal serve` with REST feature enabled and `mempal serve --mcp` remaining valid without REST.
- [ ] **Step 2:** Run the targeted CLI tests in both default and `--features rest` modes and verify the new expectations fail first.
- [ ] **Step 3:** Add a `rest` feature in `mempal-cli`, start REST on port 3080 when enabled, and have bare `serve` start MCP + REST together while `serve --mcp` keeps the P2 path.
- [ ] **Step 4:** Ensure the default build stays free of axum when the feature is disabled.
- [ ] **Step 5:** Run `cargo test -p mempal-cli` and `cargo test -p mempal-cli --features rest` to verify both build modes.
- [ ] **Step 6:** Commit: `feat(cli): integrate REST serving behind feature gate`

## Task 9: Full Verification and Cleanup

**Files:**
- Verify only

- [ ] **Step 1:** Run `cargo test --workspace` and verify all tests pass in the default build.
- [ ] **Step 2:** Run `cargo test --workspace --features rest` and verify the REST-enabled build passes.
- [ ] **Step 3:** Run `cargo clippy --workspace --all-features -- -D warnings` and verify the workspace is lint-clean.
- [ ] **Step 4:** Run a manual smoke test covering `init`, `ingest`, `search`, `wake-up`, `taxonomy`, `compress`, `serve --mcp`, and REST `GET /api/status`.
- [ ] **Step 5:** Commit: `chore: complete remaining roadmap phases`
