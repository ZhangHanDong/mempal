# P114 Coarse Workspace Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the remaining root-heavy implementation into three coarse public workspace crates while preserving the installable root `mempal` package and legacy facade paths.

**Architecture:** Keep the root package as CLI/config/API facade. Move SQLite persistence into `mempal-store-sqlite`, runtime/business logic into `mempal-runtime`, and rmcp server wiring into `mempal-mcp-server`; root modules re-export the moved APIs for compatibility.

**Tech Stack:** Rust 2024, Cargo workspace, rusqlite/sqlite-vec, rmcp, agent-spec, existing mempal crates.

## Global Constraints

- Root package remains `mempal`.
- New reusable crates must be public/publishable and must not set `publish = false`.
- Add only three coarse crates in P114: `mempal-store-sqlite`, `mempal-runtime`, `mempal-mcp-server`.
- Preserve legacy root facade paths.
- Do not change `CURRENT_SCHEMA_VERSION`; it remains `9`.
- Do not introduce fine-grained feature crates for knowledge, cowork, context, factcheck, or similar subfeatures.

---

### Task 1: Contract And Red Test

**Files:**
- Create: `specs/p114-coarse-workspace-split.spec.md`
- Create: `docs/plans/2026-07-09-p114-coarse-workspace-split.md`
- Modify: `tests/workspace_crates.rs`

**Interfaces:**
- Produces: P114 test selectors named in the spec.

- [ ] **Step 1: Add P114 workspace tests**

Add tests that assert the three coarse crates exist, remain publishable, expose direct public APIs, keep the root package installable, preserve root facade paths, and reject feature-level crate proliferation.

- [ ] **Step 2: Verify red**

Run: `cargo test --test workspace_crates p114 -- --nocapture`

Expected: FAIL because `mempal-store-sqlite`, `mempal-runtime`, and `mempal-mcp-server` do not exist yet.

- [ ] **Step 3: Validate spec quality**

Run:

```bash
agent-spec parse specs/p114-coarse-workspace-split.spec.md
agent-spec lint specs/p114-coarse-workspace-split.spec.md --min-score 0.7
agent-spec contract specs/p114-coarse-workspace-split.spec.md
```

Expected: parse succeeds with non-zero scenarios, lint score is at least 0.7, and contract renders.

### Task 2: Move SQLite Store Into `mempal-store-sqlite`

**Files:**
- Create: `crates/mempal-store-sqlite/Cargo.toml`
- Create: `crates/mempal-store-sqlite/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `src/core/db.rs`
- Modify: `src/core/mod.rs`

**Interfaces:**
- Produces: `mempal_store_sqlite::{Database, DbError, CURRENT_SCHEMA_VERSION}`.
- Preserves: `mempal::core::db::{Database, DbError, CURRENT_SCHEMA_VERSION}`.

- [ ] **Step 1: Create crate manifest**

Add `crates/mempal-store-sqlite` as a workspace member and root dependency with `version = "0.8.0"` and `path = "crates/mempal-store-sqlite"`.

- [ ] **Step 2: Move database implementation**

Move `src/core/db.rs` to `crates/mempal-store-sqlite/src/lib.rs`. Update imports from `super::anchor`, `super::types`, and `super::utils` to `mempal_agent_memory::{anchor, types::*}` and local helper functions.

- [ ] **Step 3: Preserve root facade**

Replace `src/core/db.rs` with `pub use mempal_store_sqlite::*;`.

- [ ] **Step 4: Verify store green**

Run:

```bash
cargo check -p mempal-store-sqlite
cargo test -p mempal-store-sqlite
cargo test --test workspace_crates p114_store -- --nocapture
```

Expected: all pass.

### Task 3: Move Runtime Modules Into `mempal-runtime`

**Files:**
- Create: `crates/mempal-runtime/Cargo.toml`
- Create: `crates/mempal-runtime/src/lib.rs`
- Move: `src/aaak/**`
- Move: `src/adoption_analytics.rs`
- Move: `src/brief.rs`
- Move: `src/context.rs`
- Move: `src/cowork/**`
- Move: `src/doctor.rs`
- Move: `src/factcheck/**`
- Move: `src/field_taxonomy.rs`
- Move: `src/ingest/**`
- Move: `src/knowledge_anchor.rs`
- Move: `src/knowledge_card_backfill.rs`
- Move: `src/knowledge_card_lifecycle.rs`
- Move: `src/knowledge_card_retrieval.rs`
- Move: `src/knowledge_distill.rs`
- Move: `src/knowledge_gate.rs`
- Move: `src/knowledge_lifecycle.rs`
- Move: `src/longmemeval.rs`
- Move: `src/path_filter.rs`
- Move: `src/projects.rs`
- Move: `src/search/**`
- Modify: root facade modules under `src/`

**Interfaces:**
- Produces: `mempal_runtime::{search, ingest, context, brief, cowork, factcheck, projects, doctor, aaak, ...}`.
- Consumes: `mempal_store_sqlite::Database`, `mempal_agent_memory::{anchor, types}`, and `mempal_embed`.
- Preserves: existing `mempal::<module>` root module paths.

- [ ] **Step 1: Create crate manifest**

Add `mempal-runtime` as a workspace member and root dependency.

- [ ] **Step 2: Move modules and rewrite imports**

Move runtime modules into `crates/mempal-runtime/src/`. Rewrite root-relative imports:

```rust
crate::core::db -> mempal_store_sqlite
crate::core::types -> mempal_agent_memory::types
crate::core::anchor -> mempal_agent_memory::anchor
crate::embed -> mempal_embed
crate::search -> crate::search
```

Keep runtime-local helpers such as `core::utils` inside `mempal-runtime`.

- [ ] **Step 3: Add root facade modules**

For each moved root module, keep a small module file that re-exports the runtime module, for example:

```rust
pub use mempal_runtime::search::*;
```

- [ ] **Step 4: Verify runtime green**

Run:

```bash
cargo check -p mempal-runtime
cargo test --test workspace_crates p114_runtime -- --nocapture
```

Expected: all pass.

### Task 4: Move MCP Server Into `mempal-mcp-server`

**Files:**
- Create: `crates/mempal-mcp-server/Cargo.toml`
- Create: `crates/mempal-mcp-server/src/lib.rs`
- Move: `src/mcp/server.rs`
- Move: `src/mcp/tools.rs`
- Modify: `src/mcp/mod.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `mempal_mcp_server::MempalMcpServer`.
- Consumes: `mempal_runtime`, `mempal_store_sqlite`, `mempal_mcp_protocol`, `mempal_embed`.
- Preserves: `mempal::mcp::MempalMcpServer`.

- [ ] **Step 1: Create crate manifest**

Add `mempal-mcp-server` as a workspace member and root dependency.

- [ ] **Step 2: Move MCP implementation**

Move `server.rs` and `tools.rs` into the new crate. Rewrite imports so MCP calls runtime APIs rather than root modules.

- [ ] **Step 3: Preserve root MCP facade**

Replace root `src/mcp/mod.rs` with:

```rust
pub use mempal_mcp_server::MempalMcpServer;
```

- [ ] **Step 4: Verify MCP green**

Run:

```bash
cargo check -p mempal-mcp-server
cargo test --test workspace_crates p114_root -- --nocapture
```

Expected: all pass.

### Task 5: Documentation, Inventory, And Full Verification

**Files:**
- Modify: `README.md`
- Modify: `README_zh.md`
- Modify: `docs/usage.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: completed crate layout.
- Produces: updated public docs and project inventory for P114.

- [ ] **Step 1: Update docs**

Document the new crate layout and clarify that root `mempal` remains the installable CLI package.

- [ ] **Step 2: Update spec/plan inventory**

Add P114 to `AGENTS.md` and `CLAUDE.md` completed spec and plan tables.

- [ ] **Step 3: Run full verification**

Run:

```bash
agent-spec parse specs/p114-coarse-workspace-split.spec.md
agent-spec lint specs/p114-coarse-workspace-split.spec.md --min-score 0.7
agent-spec lifecycle specs/p114-coarse-workspace-split.spec.md --code . --change-scope worktree --format json --run-log-dir .agent-spec/runs
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo package --workspace --locked --no-verify
```

Expected: all commands pass. If `cargo package` emits a pre-existing yanked transitive dependency warning, record it but do not treat it as a P114 failure unless packaging fails.

- [ ] **Step 4: Commit**

Commit with:

```bash
git add -A
git commit -m "refactor: split coarse workspace crates"
```
