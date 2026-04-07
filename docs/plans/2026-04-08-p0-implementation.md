# P0 Implementation Plan: Core → Embed → Ingest → Search+CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete init → ingest → search pipeline as a single binary.

**Architecture:** Rust workspace with 8 crates. P0 implements core (data model + SQLite), embed (ONNX embeddings), ingest (format detection + chunking + storage), search (vector retrieval + metadata filtering), and cli (init/ingest/search commands). Other crates get stub lib.rs files.

**Tech Stack:** Rust 2024, rusqlite (bundled), sqlite-vec, ort (ONNX Runtime), clap 4, tokio, serde, anyhow/thiserror

**Specs:** `specs/p0-core-scaffold.spec.md`, `specs/p0-embed-trait.spec.md`, `specs/p0-ingest.spec.md`, `specs/p0-search-cli.spec.md`

**Reference:** MemPalace Python source at `/Users/zhangalex/Work/Projects/AI/mempalace/mempalace/` and book analysis at `/Users/zhangalex/Work/Projects/AI/mempalace-book/book/src/`

**Skills:** Must use `skills/rust-skills/SKILL.md` for all Rust implementation.

---

## File Structure

```
Cargo.toml                              # workspace root
crates/
├── mempal-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # re-exports
│       ├── types.rs                    # Drawer, Triple, TaxonomyEntry, SearchResult
│       ├── db.rs                       # Database struct, open(), init schema, CRUD
│       └── config.rs                   # Config struct, load from TOML, defaults
├── mempal-embed/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # Embedder trait + re-exports
│       ├── onnx.rs                     # OnnxEmbedder (ort + MiniLM)
│       └── api.rs                      # ApiEmbedder (stub)
├── mempal-ingest/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # ingest_file(), ingest_dir()
│       ├── detect.rs                   # format detection
│       ├── normalize.rs                # multi-format → transcript
│       └── chunk.rs                    # text chunking (fixed window + QA pairs)
├── mempal-search/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # search() function
│       └── filter.rs                   # WHERE clause builder
├── mempal-cli/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                     # clap CLI: init, ingest, search, status
├── mempal-aaak/
│   ├── Cargo.toml
│   └── src/lib.rs                      # stub
├── mempal-mcp/
│   ├── Cargo.toml
│   └── src/lib.rs                      # stub
└── mempal-api/
    ├── Cargo.toml
    └── src/lib.rs                      # stub
```

---

## Task 1: Workspace Scaffold + Stub Crates

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/mempal-core/Cargo.toml`, `crates/mempal-core/src/lib.rs`
- Create: `crates/mempal-embed/Cargo.toml`, `crates/mempal-embed/src/lib.rs`
- Create: `crates/mempal-ingest/Cargo.toml`, `crates/mempal-ingest/src/lib.rs`
- Create: `crates/mempal-search/Cargo.toml`, `crates/mempal-search/src/lib.rs`
- Create: `crates/mempal-cli/Cargo.toml`, `crates/mempal-cli/src/main.rs`
- Create: `crates/mempal-aaak/Cargo.toml`, `crates/mempal-aaak/src/lib.rs`
- Create: `crates/mempal-mcp/Cargo.toml`, `crates/mempal-mcp/src/lib.rs`
- Create: `crates/mempal-api/Cargo.toml`, `crates/mempal-api/src/lib.rs`
- Create: `.gitignore`

- [ ] **Step 1:** Create workspace root `Cargo.toml` with all 8 members, edition 2024, shared workspace dependencies (rusqlite, serde, tokio, anyhow, thiserror, clap, ort, serde_json, toml)

- [ ] **Step 2:** Create each crate's `Cargo.toml` with appropriate workspace dependency references. `mempal-core` depends on rusqlite, serde, thiserror, toml. `mempal-embed` depends on mempal-core, ort, async-trait, anyhow, reqwest. `mempal-ingest` depends on mempal-core, mempal-embed, serde_json, anyhow. `mempal-search` depends on mempal-core, mempal-embed, anyhow. `mempal-cli` depends on mempal-core, mempal-embed, mempal-ingest, mempal-search, clap, tokio, anyhow. Stub crates only need minimal deps.

- [ ] **Step 3:** Create stub `lib.rs` for each library crate (just `#![warn(clippy::all)]`) and minimal `main.rs` for cli (`fn main() { println!("mempal"); }`). Create `.gitignore` with `/target`.

- [ ] **Step 4:** Run `cargo build --workspace` — must compile with zero errors.

- [ ] **Step 5:** Commit: `feat: scaffold workspace with 8 crates`

---

## Task 2: Core Data Types

**Files:**
- Create: `crates/mempal-core/src/types.rs`
- Modify: `crates/mempal-core/src/lib.rs`
- Test: `crates/mempal-core/tests/types_test.rs`

- [ ] **Step 1:** Write test in `crates/mempal-core/tests/types_test.rs`:

```rust
use mempal_core::types::*;

#[test]
fn test_drawer_fields() {
    let d = Drawer {
        id: "drawer_myapp_auth_abc12345".into(),
        content: "decided to use Clerk".into(),
        wing: "myapp".into(),
        room: Some("auth".into()),
        source_file: Some("/path/to/file.py".into()),
        source_type: SourceType::Project,
        added_at: "2026-04-08T12:00:00Z".into(),
        chunk_index: Some(0),
    };
    assert_eq!(d.wing, "myapp");
    assert!(d.room.is_some());
}

#[test]
fn test_search_result_has_citation() {
    let r = SearchResult {
        drawer_id: "d1".into(),
        content: "test".into(),
        wing: "w".into(),
        room: None,
        source_file: Some("/a.rs".into()),
        similarity: 0.95,
    };
    assert!(r.source_file.is_some());
    assert!(!r.drawer_id.is_empty());
}
```

- [ ] **Step 2:** Run test — verify it fails (types not defined yet).

Run: `cargo test -p mempal-core`

- [ ] **Step 3:** Implement `crates/mempal-core/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    Project,
    Conversation,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drawer {
    pub id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: Option<String>,
    pub source_type: SourceType,
    pub added_at: String,
    pub chunk_index: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_drawer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyEntry {
    pub wing: String,
    pub room: String,
    pub display_name: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub drawer_id: String,
    pub content: String,
    pub wing: String,
    pub room: Option<String>,
    pub source_file: Option<String>,
    pub similarity: f32,
}
```

Update `lib.rs` to `pub mod types;`

- [ ] **Step 4:** Run test — verify pass.

- [ ] **Step 5:** Commit: `feat(core): add data types — Drawer, Triple, TaxonomyEntry, SearchResult`

---

## Task 3: SQLite Database + Schema

**Files:**
- Create: `crates/mempal-core/src/db.rs`
- Modify: `crates/mempal-core/src/lib.rs`
- Test: `crates/mempal-core/tests/db_test.rs`

- [ ] **Step 1:** Write tests in `crates/mempal-core/tests/db_test.rs`:

```rust
use mempal_core::db::Database;
use tempfile::tempdir;

#[test]
fn test_db_init() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Database::open(&path).unwrap();
    // Verify tables exist
    let tables: Vec<String> = db.conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(tables.contains(&"drawers".to_string()));
    assert!(tables.contains(&"triples".to_string()));
    assert!(tables.contains(&"taxonomy".to_string()));
}

#[test]
fn test_db_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Database::open(&path).unwrap();
    db.insert_drawer(&mempal_core::types::Drawer {
        id: "test1".into(),
        content: "hello".into(),
        wing: "w".into(),
        room: None,
        source_file: None,
        source_type: mempal_core::types::SourceType::Manual,
        added_at: "2026-04-08".into(),
        chunk_index: None,
    }).unwrap();
    drop(db);
    // Reopen
    let db2 = Database::open(&path).unwrap();
    let count: i64 = db2.conn()
        .query_row("SELECT COUNT(*) FROM drawers", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 2:** Run test — verify fails.

- [ ] **Step 3:** Implement `crates/mempal-core/src/db.rs` — `Database` struct with `open()`, schema init (CREATE TABLE IF NOT EXISTS for drawers, triples, taxonomy + indexes), `insert_drawer()`, `conn()` accessor. Use `rusqlite` with `bundled` feature. Note: sqlite-vec virtual table creation deferred to Task 2B when embed crate provides dimensions.

- [ ] **Step 4:** Run tests — verify pass.

- [ ] **Step 5:** Commit: `feat(core): add Database with SQLite schema init + drawer CRUD`

---

## Task 4: Config Loading

**Files:**
- Create: `crates/mempal-core/src/config.rs`
- Modify: `crates/mempal-core/src/lib.rs`
- Test: `crates/mempal-core/tests/config_test.rs`

- [ ] **Step 1:** Write tests:

```rust
use mempal_core::config::Config;
use tempfile::tempdir;
use std::fs;

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.embed.backend, "onnx");
}

#[test]
fn test_config_load_from_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, r#"
[embed]
backend = "api"
api_endpoint = "http://localhost:11434"
"#).unwrap();
    let config = Config::load_from(&path).unwrap();
    assert_eq!(config.embed.backend, "api");
}
```

- [ ] **Step 2:** Run — verify fails.

- [ ] **Step 3:** Implement `Config` struct with `EmbedConfig` sub-struct, `Default` impl, `load()` (from `~/.mempal/config.toml`), `load_from(path)`. Use `toml` crate for deserialization, `serde::Deserialize`.

- [ ] **Step 4:** Run — verify pass.

- [ ] **Step 5:** Commit: `feat(core): add Config with TOML loading and defaults`

---

## Task 5: Embedder Trait + ONNX Implementation

**Files:**
- Create: `crates/mempal-embed/src/onnx.rs`
- Create: `crates/mempal-embed/src/api.rs`
- Modify: `crates/mempal-embed/src/lib.rs`
- Test: `crates/mempal-embed/tests/embed_test.rs`

- [ ] **Step 1:** Write trait definition in `lib.rs`:

```rust
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn name(&self) -> &str;
}
```

- [ ] **Step 2:** Write tests for empty input and dimension check:

```rust
#[tokio::test]
async fn test_embed_empty() {
    let embedder = mempal_embed::onnx::OnnxEmbedder::new_or_download().await.unwrap();
    let result = embedder.embed(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_onnx_dimensions() {
    let embedder = mempal_embed::onnx::OnnxEmbedder::new_or_download().await.unwrap();
    assert_eq!(embedder.dimensions(), 384);
}

#[tokio::test]
async fn test_onnx_embed_single() {
    let embedder = mempal_embed::onnx::OnnxEmbedder::new_or_download().await.unwrap();
    let vecs = embedder.embed(&["hello world"]).await.unwrap();
    assert_eq!(vecs.len(), 1);
    assert_eq!(vecs[0].len(), 384);
    assert!(vecs[0].iter().all(|v| *v >= -1.0 && *v <= 1.0));
}

#[tokio::test]
async fn test_onnx_batch() {
    let embedder = mempal_embed::onnx::OnnxEmbedder::new_or_download().await.unwrap();
    let vecs = embedder.embed(&["a", "b", "c"]).await.unwrap();
    assert_eq!(vecs.len(), 3);
}
```

- [ ] **Step 3:** Implement `OnnxEmbedder` in `onnx.rs` — use `ort` crate to load MiniLM ONNX model. `new_or_download()` checks `~/.mempal/models/all-MiniLM-L6-v2.onnx`, downloads from HuggingFace if missing. Implement tokenization using the model's built-in tokenizer or a simple whitespace/subword fallback.

- [ ] **Step 4:** Implement `ApiEmbedder` stub in `api.rs` — stores config, `dimensions()` returns configured value, `embed()` returns `Err("not implemented")`.

- [ ] **Step 5:** Run tests — verify pass (note: first run downloads ~80MB model).

- [ ] **Step 6:** Commit: `feat(embed): add Embedder trait + OnnxEmbedder with MiniLM`

---

## Task 6: Ingest — Format Detection + Chunking

**Files:**
- Create: `crates/mempal-ingest/src/detect.rs`
- Create: `crates/mempal-ingest/src/normalize.rs`
- Create: `crates/mempal-ingest/src/chunk.rs`
- Modify: `crates/mempal-ingest/src/lib.rs`
- Test: `crates/mempal-ingest/tests/ingest_test.rs`

- [ ] **Step 1:** Write chunking tests:

```rust
use mempal_ingest::chunk::*;

#[test]
fn test_fixed_window_chunk() {
    let text = "a".repeat(2000);
    let chunks = chunk_text(&text, 800, 100);
    assert!(chunks.len() >= 2);
    assert!(chunks[0].len() <= 800);
}

#[test]
fn test_qa_pair_chunk() {
    let transcript = "> How do I fix this?\nTry restarting.\n\n> What about the config?\nCheck settings.toml.";
    let chunks = chunk_conversation(transcript);
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].contains("How do I fix"));
    assert!(chunks[1].contains("config"));
}
```

- [ ] **Step 2:** Implement `chunk.rs` — `chunk_text(text, window, overlap)` and `chunk_conversation(transcript)`.

- [ ] **Step 3:** Write format detection tests:

```rust
use mempal_ingest::detect::*;

#[test]
fn test_detect_claude_jsonl() {
    let content = r#"{"type":"human","message":"hello"}
{"type":"assistant","message":"hi"}"#;
    assert_eq!(detect_format(content), Format::ClaudeJsonl);
}

#[test]
fn test_detect_plain_text() {
    let content = "This is a regular markdown file.";
    assert_eq!(detect_format(content), Format::PlainText);
}
```

- [ ] **Step 4:** Implement `detect.rs` — `Format` enum (ClaudeJsonl, ChatGptJson, PlainText) and `detect_format()`.

- [ ] **Step 5:** Implement `normalize.rs` — convert each format to `> user\nassistant` transcript.

- [ ] **Step 6:** Run all tests — verify pass.

- [ ] **Step 7:** Commit: `feat(ingest): add format detection, normalization, and chunking`

---

## Task 7: Ingest — Full Pipeline with Storage

**Files:**
- Modify: `crates/mempal-ingest/src/lib.rs`
- Test: `crates/mempal-ingest/tests/pipeline_test.rs`

- [ ] **Step 1:** Write integration test:

```rust
#[tokio::test]
async fn test_ingest_text_file() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = Database::open(&db_path).unwrap();
    let embedder = /* test embedder or OnnxEmbedder */;
    
    let file = dir.path().join("readme.md");
    fs::write(&file, "We decided to use PostgreSQL for the analytics database.").unwrap();
    
    let stats = ingest_file(&db, &embedder, &file, "myproject", None).await.unwrap();
    assert!(stats.chunks > 0);
    
    let count: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM drawers WHERE wing='myproject'", [], |r| r.get(0))
        .unwrap();
    assert!(count > 0);
}

#[tokio::test]
async fn test_ingest_dedup() {
    // Ingest same file twice, verify no duplicate drawers
}
```

- [ ] **Step 2:** Implement `ingest_file()` in `lib.rs` — read file → detect format → normalize → chunk → embed → write to drawers + drawer_vectors. Generate drawer_id as `drawer_{wing}_{room}_{hash8}`. Skip if drawer_id exists.

- [ ] **Step 3:** Implement `ingest_dir()` — walk directory, skip `.git`/`target`/`node_modules`, call `ingest_file` for each.

- [ ] **Step 4:** Add sqlite-vec virtual table creation in `Database::open()` — `CREATE VIRTUAL TABLE IF NOT EXISTS drawer_vectors USING vec0(...)`.

- [ ] **Step 5:** Run tests — verify pass.

- [ ] **Step 6:** Commit: `feat(ingest): complete pipeline — file → chunk → embed → SQLite`

---

## Task 8: Search Engine

**Files:**
- Modify: `crates/mempal-search/src/lib.rs`
- Create: `crates/mempal-search/src/filter.rs`
- Test: `crates/mempal-search/tests/search_test.rs`

- [ ] **Step 1:** Write search tests:

```rust
#[tokio::test]
async fn test_search_basic() {
    // Setup: insert drawers with embeddings into test DB
    // Search and verify results sorted by similarity
}

#[tokio::test]
async fn test_search_wing_filter() {
    // Insert drawers in wing_a and wing_b
    // Search with wing="wing_a", verify only wing_a results
}

#[tokio::test]
async fn test_search_empty_db() {
    // Search empty DB, expect empty results, no error
}
```

- [ ] **Step 2:** Implement `filter.rs` — build WHERE clause from optional wing/room params.

- [ ] **Step 3:** Implement `search()` in `lib.rs` — embed query → sqlite-vec distance query with optional WHERE → map to `SearchResult` with `similarity = 1.0 - distance` → sort desc.

- [ ] **Step 4:** Run tests — verify pass.

- [ ] **Step 5:** Commit: `feat(search): vector search with metadata filtering`

---

## Task 9: CLI — init, ingest, search

**Files:**
- Modify: `crates/mempal-cli/src/main.rs`
- Test: manual CLI testing + `crates/mempal-cli/tests/cli_test.rs`

- [ ] **Step 1:** Implement CLI with clap:

```rust
#[derive(Parser)]
#[command(name = "mempal", about = "Project memory for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init { dir: PathBuf },
    Ingest { dir: PathBuf, #[arg(long)] wing: String, #[arg(long)] format: Option<String> },
    Search { query: String, #[arg(long)] wing: Option<String>, #[arg(long)] room: Option<String>, #[arg(long, default_value = "10")] top_k: usize, #[arg(long)] json: bool },
    Status,
}
```

- [ ] **Step 2:** Implement `init` — scan dir for subdirectories, create taxonomy entries, print detected rooms.

- [ ] **Step 3:** Implement `ingest` — call `ingest_dir()`, print stats.

- [ ] **Step 4:** Implement `search` — call `search()`, print results (human-readable or JSON).

- [ ] **Step 5:** Implement `status` — query drawers count, wing/room stats, print summary.

- [ ] **Step 6:** Test manually: `cargo run -p mempal-cli -- init /tmp/testproject`

- [ ] **Step 7:** Write E2E test:

```rust
#[tokio::test]
async fn test_e2e_init_ingest_search() {
    // Create temp dir with README.md containing "decided to use PostgreSQL"
    // Run init, ingest, search programmatically
    // Verify search finds the README content
}
```

- [ ] **Step 8:** Commit: `feat(cli): add init, ingest, search, status commands`

---

## Task 10: Final Integration + Cleanup

- [ ] **Step 1:** Run full test suite: `cargo test --workspace`

- [ ] **Step 2:** Run clippy: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3:** Fix any clippy warnings.

- [ ] **Step 4:** Run manual E2E: `cargo run -p mempal-cli -- init . && cargo run -p mempal-cli -- ingest . --wing mempal && cargo run -p mempal-cli -- search "workspace scaffold"`

- [ ] **Step 5:** Commit: `chore: P0 complete — init/ingest/search pipeline working`

---

## Verification Checklist (from specs)

After P0 completion, verify each spec scenario:

- [ ] `cargo build --workspace` — zero errors
- [ ] `Database::open()` creates tables + indexes
- [ ] `Database::open()` on existing DB preserves data
- [ ] `Config::load()` reads TOML, defaults work
- [ ] `Drawer` has all required fields
- [ ] `OnnxEmbedder` generates 384-dim vectors
- [ ] Batch embed returns correct count
- [ ] Empty embed returns empty vec
- [ ] Text file ingest creates drawers + vectors
- [ ] Code file ingest creates 2+ chunks with overlap
- [ ] Claude JSONL parsed and chunked by QA pairs
- [ ] Duplicate ingest is idempotent
- [ ] Empty file is skipped
- [ ] Directory ingest ignores .git/target/node_modules
- [ ] Search returns results sorted by similarity
- [ ] Wing filter works
- [ ] Wing+Room filter works
- [ ] Search results contain source_file and drawer_id
- [ ] `--json` outputs valid JSON
- [ ] Empty DB search returns empty, no error
- [ ] `init` detects rooms from directory structure
- [ ] `ingest` prints stats
- [ ] E2E: init → ingest → search finds content
