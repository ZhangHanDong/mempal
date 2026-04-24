# P14 Context Assembler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `mempal context` as the first runtime assembler for the mind-model stack.

**Architecture:** Add a focused `src/context.rs` module that builds a `ContextPack` from existing typed drawers. The CLI should remain thin: parse flags, call the assembler, render plain or JSON. No schema bump, no MCP/REST surface, no skill execution.

**Tech Stack:** Rust 2024, clap, serde, rusqlite-backed `Database`, existing `search` and `anchor` modules, integration tests under `tests/context_assembler.rs`.

---

## Source Spec

- `specs/p14-context-assembler.spec.md`

## File Map

- Create `src/context.rs`: owns `ContextRequest`, `ContextPack`, `ContextSection`, `ContextItem`, assembly order, status eligibility, anchor precedence, and text selection.
- Modify `src/lib.rs`: export `context`.
- Modify `src/main.rs`: add `Commands::Context`, parse CLI flags, initialize embedder, call assembler, render `plain` / `json`.
- Modify `src/core/db.rs`: add read-only helper if search filters cannot express anchor-id precedence cleanly.
- Modify `src/search/**`: only if needed to avoid duplicating filter behavior; do not change ranking.
- Create `tests/context_assembler.rs`: integration tests for all P14 scenarios.
- Modify `docs/MIND-MODEL-DESIGN.md`: mark P14 as Phase-1 runtime assembler work if implementation decisions diverge from the design doc.
- Modify `AGENTS.md` / `CLAUDE.md`: move P14 from current draft to completed only after all checks pass.

## Task 1: Test Harness And First Failing Scenario

**Files:**
- Create: `tests/context_assembler.rs`

- [ ] **Step 1: Add integration test helpers**

Create helper functions that open a temporary database, insert typed drawers, and run the CLI binary using `assert_cmd`-style patterns already used in existing tests. If no command-runner helper exists, use the existing project pattern from `tests/mind_model_bootstrap.rs`.

Minimum fixture shape:

```rust
fn knowledge_drawer(
    id: &str,
    tier: KnowledgeTier,
    status: KnowledgeStatus,
    statement: &str,
    content: &str,
) -> Drawer {
    // Build a Drawer with memory_kind=Knowledge and repo anchor.
}
```

- [ ] **Step 2: Write tier-order failing test**

Add:

```rust
#[test]
fn test_context_groups_knowledge_by_tier_order() {
    // Insert dao_tian, dao_ren, shu, qi matching the same query/domain/field.
    // Run `mempal context "debug failing build" --field software-engineering`.
    // Assert section order and citation metadata.
}
```

- [ ] **Step 3: Run targeted test and verify failure**

Run:

```bash
cargo test --test context_assembler test_context_groups_knowledge_by_tier_order -- --exact
```

Expected: FAIL because `mempal context` does not exist.

## Task 2: Core Context Types And Assembly Skeleton

**Files:**
- Create: `src/context.rs`
- Modify: `src/lib.rs`
- Test: `tests/context_assembler.rs`

- [ ] **Step 1: Define public internal types**

Implement:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ContextRequest {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub cwd: PathBuf,
    pub include_evidence: bool,
    pub max_items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    pub query: String,
    pub domain: MemoryDomain,
    pub field: String,
    pub anchors: Vec<ContextAnchor>,
    pub sections: Vec<ContextSection>,
}
```

Include `ContextSection`, `ContextItem`, and `ContextAnchor`. Keep these data-only and serializable.

- [ ] **Step 2: Add deterministic tier/status helpers**

Implement helpers:

```rust
fn tier_order() -> [KnowledgeTier; 4]
fn is_active_status(status: &KnowledgeStatus) -> bool
fn context_text(drawer: &Drawer) -> &str
```

Rules:
- active status is `Canonical` or `Promoted`
- knowledge text prefers non-empty `statement`
- evidence text uses `content`

- [ ] **Step 3: Add a minimal assembler function**

Implement a function shaped like:

```rust
pub async fn assemble_context<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    request: ContextRequest,
) -> Result<ContextPack, ContextError>
```

The first version can return tier sections using existing search filters and no anchor precedence yet.

- [ ] **Step 4: Export module**

Modify `src/lib.rs`:

```rust
pub mod context;
```

- [ ] **Step 5: Run unit/API test**

Run:

```bash
cargo test --test context_assembler test_context_assembler_returns_typed_pack -- --exact
```

Expected: PASS after adding the dedicated test and implementation.

## Task 3: Anchor Precedence, Domain/Field Filters, And Evidence Gate

**Files:**
- Modify: `src/context.rs`
- Modify: `src/core/db.rs` only if needed
- Test: `tests/context_assembler.rs`

- [ ] **Step 1: Write anchor precedence test**

Add `test_context_prefers_worktree_anchor_before_repo_and_global`.

Expected failure before implementation: same-tier output does not sort by anchor precedence.

- [ ] **Step 2: Implement anchor derivation**

Use `anchor::derive_anchor_from_cwd(Some(&request.cwd))`. The resulting active anchor list should be:

```text
worktree://...
repo://...
repo://legacy
global://...
```

Use the derived repo anchor when `parent_anchor_id` exists, and keep `repo://legacy` as a compatibility fallback for drawers backfilled by P12 migrations. Global should be represented as `anchor_kind=Global`; use `global://default` or the existing canonical global id if already present. Global anchor candidates must query `domain=global`, not the request domain.

- [ ] **Step 3: Apply domain/field/tier/status filters**

For each tier:
- `memory_kind=knowledge`
- `domain=request.domain`
- `field=request.field`
- `tier=<current tier>`
- `status in canonical/promoted`

If existing `SearchFilters` cannot express multiple allowed statuses, run one search per allowed status and merge/dedup before sorting.

- [ ] **Step 4: Apply anchor precedence after retrieval**

Sort same-tier items by:

```text
anchor_rank(worktree)=0
anchor_rank(repo)=1
anchor_rank(global)=2
search_score ascending/descending according to existing score semantics
drawer_id stable tiebreak
```

Do not modify search ranking internals.

- [ ] **Step 5: Implement evidence gate**

Only add evidence section when `request.include_evidence == true`. Evidence filters:
- `memory_kind=evidence`
- `domain=request.domain`
- `field=request.field`
- matching active anchors

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test --test context_assembler test_context_prefers_worktree_anchor_before_repo_and_global -- --exact
cargo test --test context_assembler test_context_domain_and_field_filters_exclude_unrelated_knowledge -- --exact
cargo test --test context_assembler test_context_omits_evidence_by_default -- --exact
cargo test --test context_assembler test_context_include_evidence_adds_evidence_section_after_qi -- --exact
```

Expected: PASS.

## Task 4: CLI Surface And Renderers

**Files:**
- Modify: `src/main.rs`
- Test: `tests/context_assembler.rs`

- [ ] **Step 1: Add `Commands::Context`**

Add clap args:

```rust
Context {
    query: String,
    #[arg(long, default_value = "general")]
    field: String,
    #[arg(long, default_value = "project")]
    domain: String,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value = "plain")]
    format: String,
    #[arg(long)]
    include_evidence: bool,
    #[arg(long, default_value_t = 12)]
    max_items: usize,
}
```

- [ ] **Step 2: Add validation**

Reject:
- `max_items == 0`
- unsupported `format`
- unsupported `domain`

Use `bail!` with clear messages.

- [ ] **Step 3: Implement plain renderer**

Plain output should group sections:

```text
## dao_tian
- <text>
  source: <source_file>
  drawer: <drawer_id>
```

Include `trigger_hints` only as metadata text for items that have it. Do not execute skills.

- [ ] **Step 4: Implement JSON renderer**

Use `serde_json::to_string_pretty(&pack)` and print to stdout.

- [ ] **Step 5: Run CLI tests**

Run:

```bash
cargo test --test context_assembler test_context_json_output_exposes_stable_pack_shape -- --exact
cargo test --test context_assembler test_context_empty_result_exits_successfully -- --exact
cargo test --test context_assembler test_context_rejects_invalid_max_items -- --exact
```

Expected: PASS.

## Task 5: Statement Selection, Inactive Statuses, And Schema Guard

**Files:**
- Modify: `src/context.rs`
- Test: `tests/context_assembler.rs`

- [ ] **Step 1: Add statement/content regression tests**

Add or complete:
- `test_context_knowledge_item_uses_statement_before_content`
- `test_context_excludes_inactive_knowledge_statuses`
- `test_context_assembler_does_not_bump_schema`

- [ ] **Step 2: Implement missing behavior**

Ensure:
- blank `statement` falls back to `content`
- `candidate`, `demoted`, and `retired` are excluded
- schema version remains v7
- no tables are created by context assembly

- [ ] **Step 3: Run targeted tests**

Run:

```bash
cargo test --test context_assembler test_context_knowledge_item_uses_statement_before_content -- --exact
cargo test --test context_assembler test_context_excludes_inactive_knowledge_statuses -- --exact
cargo test --test context_assembler test_context_assembler_does_not_bump_schema -- --exact
```

Expected: PASS.

## Task 6: Documentation, Verification, And Commit

**Files:**
- Modify: `docs/MIND-MODEL-DESIGN.md` if implementation clarifies Phase-1 runtime behavior
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update docs status**

After implementation passes, move `specs/p14-context-assembler.spec.md` from current draft to completed in `AGENTS.md` and `CLAUDE.md`. Add this plan to the implementation plan list as completed.

- [ ] **Step 2: Run contract validation**

Run:

```bash
agent-spec parse specs/p14-context-assembler.spec.md
agent-spec lint specs/p14-context-assembler.spec.md --min-score 0.7
```

Expected: parse succeeds, lint score >= 70%.

- [ ] **Step 3: Run Rust verification**

Run:

```bash
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

Expected: all pass.

- [ ] **Step 4: Commit**

Run:

```bash
git add specs/p14-context-assembler.spec.md docs/plans/2026-04-24-p14-context-assembler-implementation.md src/context.rs src/lib.rs src/main.rs src/core/db.rs tests/context_assembler.rs AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
git commit -m "feat: add mind-model context assembler"
```

- [ ] **Step 5: Save decision memory**

Use `mempal_ingest` or the current source-built CLI if the installed binary lags schema v7. Record what shipped, why `mempal context` is CLI-first, and which items remain out of scope.
