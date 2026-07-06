# P111 Implementation Plan - project ignore rules for init and ingest

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `mempal init` and directory-mode `mempal ingest` share one project path-filter layer that respects `.gitignore`, `.mempalignore`, and explicit custom ignore files.

**Architecture:** Add a small `src/path_filter.rs` module that configures the `ignore` crate once and exposes a shared traversal policy. Wire that policy into `init` room detection and `ingest_dir_with_options`, keeping explicit file ingest unchanged.

**Tech Stack:** Rust 2024, `ignore = "0.4"`, clap 4, existing `anyhow`/`thiserror`, existing tempfile-based tests.

## Global Constraints

- No schema migration, database table, or database column.
- No rewrite, delete, or re-embed of existing drawers.
- Shared path filtering must be used by both `mempal init` and directory-mode `mempal ingest`.
- Default traversal respects Git ignore rules and root `.mempalignore`.
- Hard traversal skips for `.git`, `target`, and `node_modules` remain.
- Explicit file ingest is not filtered by project ignore rules.

---

## File Structure

- `Cargo.toml` / `Cargo.lock`: add `ignore = "0.4"`.
- `src/path_filter.rs`: new shared filter options, walker builder, hard-skip helper, and error type.
- `src/lib.rs`: export `path_filter`.
- `src/main.rs`: add `init`/`ingest` flags and use the shared filter in `detect_rooms`.
- `src/ingest/mod.rs`: add ignore options to `IngestOptions` and use shared traversal for directory ingest.
- `tests/ingest_lock.rs`: directory ingest regression tests.
- `README.md`, `README_zh.md`, `docs/usage.md`: document defaults and override flags.
- `AGENTS.md`, `CLAUDE.md`: inventory P111 spec/plan.

---

### Task 1: Shared Path Filter Module

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/lib.rs`
- Create: `src/path_filter.rs`

**Interfaces:**
- Produces:
  - `ProjectPathFilterOptions`
  - `ProjectPathFilterError`
  - `project_walk(root, options) -> Result<ignore::Walk, ProjectPathFilterError>`
  - `is_hard_skipped_dir(path) -> bool`

- [ ] **Step 1: Add the dependency**

Edit `Cargo.toml` dependencies alphabetically:

```toml
ignore = "0.4"
```

Run:

```bash
cargo check
```

Expected: Cargo resolves `ignore` and updates `Cargo.lock`.

- [ ] **Step 2: Create failing unit tests in `src/path_filter.rs`**

Create `src/path_filter.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &std::path::Path, content: &str) {
        fs::write(path, content).expect("write fixture");
    }

    #[test]
    fn default_walk_respects_gitignore_and_mempalignore() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join("src/auth")).expect("src");
        fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        fs::create_dir_all(root.join("target/release-room")).expect("target");
        write(&root.join(".gitignore"), "dist/\n");
        write(&root.join(".mempalignore"), "generated/\n");

        let paths = collect_relative_paths(root, &ProjectPathFilterOptions::default())
            .expect("walk succeeds");

        assert!(paths.iter().any(|path| path == "src/auth"));
        assert!(!paths.iter().any(|path| path.starts_with("dist/")));
        assert!(!paths.iter().any(|path| path.starts_with("generated/")));
        assert!(!paths.iter().any(|path| path.starts_with("target/")));
    }

    #[test]
    fn no_gitignore_keeps_mempalignore() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        write(&root.join(".gitignore"), "dist/\n");
        write(&root.join(".mempalignore"), "generated/\n");

        let paths = collect_relative_paths(
            root,
            &ProjectPathFilterOptions {
                respect_gitignore: false,
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("walk succeeds");

        assert!(paths.iter().any(|path| path.starts_with("dist/")));
        assert!(!paths.iter().any(|path| path.starts_with("generated/")));
    }

    #[test]
    fn custom_ignore_file_is_validated_and_applied() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        let custom = root.join("custom.ignore");
        fs::create_dir_all(root.join("notes/api")).expect("notes");
        write(&custom, "notes/\n");

        let paths = collect_relative_paths(
            root,
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![custom],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("walk succeeds");

        assert!(!paths.iter().any(|path| path.starts_with("notes/")));
    }

    #[test]
    fn missing_custom_ignore_file_is_an_error() {
        let tmp = TempDir::new().expect("tempdir");
        let missing = tmp.path().join("missing.ignore");
        let err = collect_relative_paths(
            tmp.path(),
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![missing.clone()],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect_err("missing ignore file should fail");

        assert!(err.to_string().contains(&missing.display().to_string()));
    }
}
```

Run:

```bash
cargo test --lib path_filter
```

Expected: FAIL because the module API is not implemented.

- [ ] **Step 3: Implement the module**

Implement:

```rust
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPathFilterOptions {
    pub respect_gitignore: bool,
    pub respect_mempalignore: bool,
    pub custom_ignore_files: Vec<PathBuf>,
}

impl Default for ProjectPathFilterOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            respect_mempalignore: true,
            custom_ignore_files: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProjectPathFilterError {
    #[error("custom ignore file does not exist: {path}")]
    MissingCustomIgnoreFile { path: PathBuf },
    #[error("project traversal failed for {path}")]
    Walk {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },
}

pub fn project_walk(
    root: &Path,
    options: &ProjectPathFilterOptions,
) -> Result<ignore::Walk, ProjectPathFilterError> {
    for path in &options.custom_ignore_files {
        if !path.is_file() {
            return Err(ProjectPathFilterError::MissingCustomIgnoreFile { path: path.clone() });
        }
    }

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(options.respect_gitignore)
        .git_exclude(options.respect_gitignore)
        .git_global(options.respect_gitignore)
        .parents(true);

    if options.respect_mempalignore {
        builder.add_custom_ignore_filename(".mempalignore");
    }

    for path in &options.custom_ignore_files {
        builder.add_ignore(path);
    }

    builder.filter_entry(|entry| !is_hard_skipped_dir(entry.path()));
    Ok(builder.build())
}

pub fn is_hard_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| matches!(name, ".git" | "target" | "node_modules"))
        .unwrap_or(false)
}

pub fn collect_relative_paths(
    root: &Path,
    options: &ProjectPathFilterOptions,
) -> Result<Vec<String>, ProjectPathFilterError> {
    let mut paths = Vec::new();
    for entry in project_walk(root, options)? {
        let entry = entry.map_err(|source| ProjectPathFilterError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        if entry.path() == root {
            continue;
        }
        if let Ok(relative) = entry.path().strip_prefix(root) {
            paths.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    paths.sort();
    Ok(paths)
}
```

- [ ] **Step 4: Export the module**

Add to `src/lib.rs`:

```rust
pub mod path_filter;
```

- [ ] **Step 5: Verify**

Run:

```bash
cargo test --lib path_filter
cargo check
```

Expected: PASS.

---

### Task 2: Wire Ignore Flags Into `mempal init`

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `mempal::path_filter::{ProjectPathFilterOptions, project_walk}`
- Produces: `InitIgnoreArgs` and `detect_rooms_with_options`

- [ ] **Step 1: Add failing init tests**

Append tests under `#[cfg(test)] mod tests` in `src/main.rs`:

```rust
#[test]
fn test_init_detect_rooms_respects_gitignore_and_mempalignore() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src/auth")).expect("src auth");
    std::fs::create_dir_all(root.join("dist/build-room")).expect("dist");
    std::fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
    std::fs::create_dir_all(root.join("target/release-room")).expect("target");
    std::fs::create_dir_all(root.join("node_modules/pkg-room")).expect("node_modules");
    std::fs::write(root.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(root.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let rooms = detect_rooms_with_options(root, &ProjectPathFilterOptions::default())
        .expect("detect rooms");

    assert!(rooms.contains(&"auth".to_string()));
    assert!(!rooms.contains(&"build-room".to_string()));
    assert!(!rooms.contains(&"gen-room".to_string()));
    assert!(!rooms.contains(&"release-room".to_string()));
    assert!(!rooms.contains(&"pkg-room".to_string()));
}

#[test]
fn test_init_no_gitignore_keeps_mempalignore() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("dist/build-room")).expect("dist");
    std::fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
    std::fs::write(root.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(root.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let rooms = detect_rooms_with_options(
        root,
        &ProjectPathFilterOptions {
            respect_gitignore: false,
            ..ProjectPathFilterOptions::default()
        },
    )
    .expect("detect rooms");

    assert!(rooms.contains(&"build-room".to_string()));
    assert!(!rooms.contains(&"gen-room".to_string()));
}

#[test]
fn test_init_custom_ignore_file_excludes_rooms() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path();
    let custom = root.join("custom.ignore");
    std::fs::create_dir_all(root.join("notes/api")).expect("notes api");
    std::fs::write(&custom, "notes/\n").expect("custom ignore");

    let rooms = detect_rooms_with_options(
        root,
        &ProjectPathFilterOptions {
            custom_ignore_files: vec![custom],
            ..ProjectPathFilterOptions::default()
        },
    )
    .expect("detect rooms");

    assert!(!rooms.contains(&"api".to_string()));
}

#[test]
fn test_init_missing_ignore_file_fails() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let missing = tmp.path().join("missing.ignore");
    let err = detect_rooms_with_options(
        tmp.path(),
        &ProjectPathFilterOptions {
            custom_ignore_files: vec![missing.clone()],
            ..ProjectPathFilterOptions::default()
        },
    )
    .expect_err("missing custom ignore file should fail");

    assert!(err.to_string().contains(&missing.display().to_string()));
}
```

Run:

```bash
cargo test --bin mempal test_init_detect_rooms_respects_gitignore_and_mempalignore
```

Expected: FAIL because `ProjectPathFilterOptions` is not imported and `detect_rooms_with_options` does not exist.

- [ ] **Step 2: Add CLI args**

Change `Commands::Init`:

```rust
Init {
    dir: PathBuf,
    #[arg(long)]
    dry_run: bool,
    #[arg(long = "ignore-file")]
    ignore_files: Vec<PathBuf>,
    #[arg(long)]
    no_gitignore: bool,
    #[arg(long)]
    no_mempalignore: bool,
},
```

Update the match arm:

```rust
Commands::Init {
    dir,
    dry_run,
    ignore_files,
    no_gitignore,
    no_mempalignore,
} => init_command(
    &db,
    &dir,
    dry_run,
    ProjectPathFilterOptions {
        respect_gitignore: !no_gitignore,
        respect_mempalignore: !no_mempalignore,
        custom_ignore_files: ignore_files,
    },
),
```

- [ ] **Step 3: Replace `detect_rooms` traversal**

Import:

```rust
use mempal::path_filter::{ProjectPathFilterOptions, project_walk};
```

Change signatures:

```rust
fn init_command(
    db: &Database,
    dir: &Path,
    dry_run: bool,
    filter_options: ProjectPathFilterOptions,
) -> Result<()> {
    ...
    let rooms = detect_rooms_with_options(dir, &filter_options)?;
    ...
}

fn detect_rooms_with_options(
    dir: &Path,
    filter_options: &ProjectPathFilterOptions,
) -> Result<Vec<String>> {
    let mut rooms = BTreeSet::new();
    for entry in project_walk(dir, filter_options)? {
        let entry = entry.with_context(|| format!("failed to walk {}", dir.display()))?;
        let path = entry.path();
        if path == dir || !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && !matches!(name, "src" | "tests")
        {
            rooms.insert(name.to_string());
        }
    }
    Ok(rooms.into_iter().collect())
}
```

Remove the old `should_skip_dir` helper in `src/main.rs`.

- [ ] **Step 4: Verify init tests**

Run:

```bash
cargo test --bin mempal test_init_detect_rooms_respects_gitignore_and_mempalignore
cargo test --bin mempal test_init_no_gitignore_keeps_mempalignore
cargo test --bin mempal test_init_custom_ignore_file_excludes_rooms
cargo test --bin mempal test_init_missing_ignore_file_fails
```

Expected: PASS.

---

### Task 3: Wire Shared Filtering Into Directory Ingest

**Files:**
- Modify: `src/ingest/mod.rs`
- Modify: `src/main.rs`
- Modify: `tests/ingest_lock.rs`

**Interfaces:**
- Consumes: `ProjectPathFilterOptions`
- Produces: `IngestOptions.project_filter`

- [ ] **Step 1: Add failing ingest tests**

Add to `tests/ingest_lock.rs`:

```rust
#[test]
fn test_ingest_dir_respects_gitignore_and_mempalignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir(&source_dir).expect("source dir");

    write_file(&source_dir, "keep.md", "keep this document");
    std::fs::create_dir_all(source_dir.join("dist")).expect("dist");
    std::fs::create_dir_all(source_dir.join("generated")).expect("generated");
    std::fs::create_dir_all(source_dir.join("target")).expect("target");
    write_file(&source_dir.join("dist"), "ignored.md", "git ignored");
    write_file(&source_dir.join("generated"), "ignored.md", "mempal ignored");
    write_file(&source_dir.join("target"), "ignored.md", "hard ignored");
    std::fs::write(source_dir.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(source_dir.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    assert_eq!(stats.chunks, 1);
    let drawers = db.all_active_drawers().expect("drawers");
    assert_eq!(drawers.len(), 1);
    assert_eq!(drawers[0].4.as_deref(), Some("keep.md"));
}

#[test]
fn test_ingest_no_mempalignore_keeps_gitignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir_all(source_dir.join("dist")).expect("dist");
    std::fs::create_dir_all(source_dir.join("generated")).expect("generated");
    write_file(&source_dir.join("dist"), "ignored.md", "git ignored");
    write_file(&source_dir.join("generated"), "remember.md", "mempal ignore disabled");
    std::fs::write(source_dir.join(".gitignore"), "dist/\n").expect("gitignore");
    std::fs::write(source_dir.join(".mempalignore"), "generated/\n").expect("mempalignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                project_filter: ProjectPathFilterOptions {
                    respect_mempalignore: false,
                    ..ProjectPathFilterOptions::default()
                },
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    let drawers = db.all_active_drawers().expect("drawers");
    assert_eq!(drawers[0].4.as_deref(), Some("generated/remember.md"));
}

#[test]
fn test_ingest_custom_ignore_file_excludes_sources() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let source_dir = tmp.path().join("source");
    let custom = source_dir.join("custom.ignore");
    std::fs::create_dir_all(source_dir.join("notes")).expect("notes");
    write_file(&source_dir, "keep.md", "keep this");
    write_file(&source_dir.join("notes"), "ignored.md", "ignore this");
    std::fs::write(&custom, "notes/\n").expect("custom ignore");

    let stats = rt
        .block_on(ingest_dir_with_options(
            &db,
            &StubEmbedder,
            &source_dir,
            "test",
            IngestOptions {
                room: Some("docs"),
                source_root: Some(&source_dir),
                project_filter: ProjectPathFilterOptions {
                    custom_ignore_files: vec![custom],
                    ..ProjectPathFilterOptions::default()
                },
                ..IngestOptions::default()
            },
        ))
        .expect("ingest dir");

    assert_eq!(stats.files, 1);
    let drawers = db.all_active_drawers().expect("drawers");
    assert_eq!(drawers[0].4.as_deref(), Some("keep.md"));
}
```

Run one filter:

```bash
cargo test --test ingest_lock test_ingest_dir_respects_gitignore_and_mempalignore
```

Expected: FAIL because `IngestOptions.project_filter` does not exist.

- [ ] **Step 2: Extend `IngestOptions`**

In `src/ingest/mod.rs`:

```rust
use crate::path_filter::{ProjectPathFilterOptions, project_walk};
```

Add field:

```rust
pub project_filter: ProjectPathFilterOptions,
```

Because `IngestOptions` is `Copy` today and `ProjectPathFilterOptions` owns a
`Vec<PathBuf>`, remove `Copy` from `IngestOptions`:

```rust
#[derive(Debug, Clone, Default)]
pub struct IngestOptions<'a> {
    ...
    pub project_filter: ProjectPathFilterOptions,
}
```

Update all `IngestOptions { ... }` construction sites to use
`..IngestOptions::default()` where possible, or explicitly set
`project_filter: ProjectPathFilterOptions::default()`.

- [ ] **Step 3: Use shared traversal in `ingest_dir_with_options`**

Replace the manual stack/read_dir loop with:

```rust
for entry in project_walk(dir, &options.project_filter).map_err(|source| IngestError::ReadDir {
    path: dir.to_path_buf(),
    source: std::io::Error::new(std::io::ErrorKind::Other, source),
})? {
    let entry = entry.map_err(|source| IngestError::ReadDirEntry {
        path: dir.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::Other, source),
    })?;
    let path = entry.path();
    if path == dir || path.is_dir() {
        continue;
    }
    if path.is_file() {
        if should_skip_file(path) {
            stats.skipped += 1;
            continue;
        }
        let file_stats = ingest_file_with_options(db, embedder, path, wing, options.clone()).await?;
        stats.files += file_stats.files;
        stats.chunks += file_stats.chunks;
        stats.skipped += file_stats.skipped;
        stats.noise_bytes_stripped =
            merge_optional_sum(stats.noise_bytes_stripped, file_stats.noise_bytes_stripped);
    }
}
```

If the `std::io::Error` mapping is too lossy, add new `IngestError` variants
for `ProjectPathFilterError` and `ignore::Error` instead:

```rust
#[error("failed to apply project ignore rules")]
ProjectFilter(#[from] crate::path_filter::ProjectPathFilterError),
#[error("failed to walk project path {path}")]
WalkPath {
    path: PathBuf,
    #[source]
    source: ignore::Error,
},
```

- [ ] **Step 4: Add ingest CLI flags**

Change `Commands::Ingest`:

```rust
#[arg(long = "ignore-file")]
ignore_files: Vec<PathBuf>,
#[arg(long)]
no_gitignore: bool,
#[arg(long)]
no_mempalignore: bool,
```

Pass to `IngestOptions` in `ingest_command`:

```rust
project_filter: ProjectPathFilterOptions {
    respect_gitignore: !args.no_gitignore,
    respect_mempalignore: !args.no_mempalignore,
    custom_ignore_files: args.ignore_files.clone(),
},
```

Keep direct file ingest unchanged; the project filter only affects
`ingest_dir_with_options`.

- [ ] **Step 5: Add explicit file bypass test**

Add:

```rust
#[tokio::test]
async fn test_ingest_explicit_file_bypasses_project_ignore() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("palace.db");
    let db = Database::open(&db_path).expect("init db");
    let dist = tmp.path().join("dist");
    std::fs::create_dir(&dist).expect("dist");
    std::fs::write(tmp.path().join(".gitignore"), "dist/\n").expect("gitignore");
    let file = write_file(&dist, "manual.md", "explicit file still ingests");

    let stats = ingest_file_with_options(
        &db,
        &StubEmbedder,
        &file,
        "test",
        IngestOptions {
            room: Some("docs"),
            source_root: Some(tmp.path()),
            ..IngestOptions::default()
        },
    )
    .await
    .expect("explicit ingest");

    assert_eq!(stats.files, 1);
    assert_eq!(stats.chunks, 1);
    assert_eq!(db.drawer_count().expect("drawer count"), 1);
}
```

- [ ] **Step 6: Verify ingest tests**

Run:

```bash
cargo test --test ingest_lock test_ingest_dir_respects_gitignore_and_mempalignore
cargo test --test ingest_lock test_ingest_no_mempalignore_keeps_gitignore
cargo test --test ingest_lock test_ingest_custom_ignore_file_excludes_sources
cargo test --test ingest_lock test_ingest_explicit_file_bypasses_project_ignore
```

Expected: PASS.

---

### Task 4: CLI Parse Coverage and Usage Docs

**Files:**
- Modify: `src/main.rs`
- Modify: `README.md`
- Modify: `README_zh.md`
- Modify: `docs/usage.md`

**Interfaces:**
- Consumes: `--ignore-file`, `--no-gitignore`, `--no-mempalignore` from Tasks 2-3.

- [ ] **Step 1: Add CLI parse tests**

Add tests in `src/main.rs`:

```rust
#[test]
fn test_cli_init_ignore_flags_parse() {
    let cli = Cli::parse_from([
        "mempal",
        "init",
        ".",
        "--dry-run",
        "--ignore-file",
        "extra.ignore",
        "--no-gitignore",
        "--no-mempalignore",
    ]);
    match cli.command {
        Commands::Init {
            dry_run,
            ignore_files,
            no_gitignore,
            no_mempalignore,
            ..
        } => {
            assert!(dry_run);
            assert_eq!(ignore_files, vec![PathBuf::from("extra.ignore")]);
            assert!(no_gitignore);
            assert!(no_mempalignore);
        }
        _ => panic!("expected init"),
    }
}

#[test]
fn test_cli_ingest_ignore_flags_parse() {
    let cli = Cli::parse_from([
        "mempal",
        "ingest",
        ".",
        "--wing",
        "demo",
        "--ignore-file",
        "extra.ignore",
        "--no-gitignore",
        "--no-mempalignore",
    ]);
    match cli.command {
        Commands::Ingest {
            ignore_files,
            no_gitignore,
            no_mempalignore,
            ..
        } => {
            assert_eq!(ignore_files, vec![PathBuf::from("extra.ignore")]);
            assert!(no_gitignore);
            assert!(no_mempalignore);
        }
        _ => panic!("expected ingest"),
    }
}
```

Run:

```bash
cargo test --bin mempal test_cli_init_ignore_flags_parse
cargo test --bin mempal test_cli_ingest_ignore_flags_parse
```

Expected: PASS.

- [ ] **Step 2: Document behavior in README files**

Add a short section near init/ingest usage:

```markdown
### Project ignore rules

`mempal init` and directory-mode `mempal ingest` respect the target project's
`.gitignore` and a root `.mempalignore` by default. Use `.mempalignore` for
paths that should not enter project memory even if they are tracked or useful
to other tools.

```bash
mempal init . --dry-run
mempal ingest . --wing my-project --ignore-file extra.ignore
```

Use `--no-gitignore` or `--no-mempalignore` to disable one ignore source for a
single run. Explicit file ingest remains explicit: `mempal ingest path/file.md`
reads that file even if a project-root directory traversal would ignore it.
```

Mirror the same content in `README_zh.md` in Chinese.

- [ ] **Step 3: Document behavior in `docs/usage.md`**

Add an operational note:

```markdown
## Project ignore rules

P111 traversal uses the same filter for taxonomy initialization and directory
ingest. Defaults:

- respect `.gitignore`, `.git/info/exclude`, and global Git excludes;
- respect root `.mempalignore`;
- hard-skip `.git/`, `target/`, and `node_modules/`;
- apply custom ignore files passed with repeated `--ignore-file`.

The filter only applies to directory traversal. Explicit file ingest is treated
as intentional input.
```

- [ ] **Step 4: Verify docs mention both entry points**

Run:

```bash
rg "mempalignore|no-gitignore|ignore-file" README.md README_zh.md docs/usage.md
```

Expected: each file mentions `.mempalignore` and at least one ignore flag.

---

### Task 5: Inventory and Full Verification

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Move P111 from current spec to completed spec inventory**

After implementation passes, update both files:

```markdown
| `specs/p111-project-ignore-rules.spec.md` | 完成 | P111 project ignore rules：`init` / directory `ingest` 共享 project path-filter，默认尊重 `.gitignore` + `.mempalignore`，支持 `--ignore-file` / `--no-gitignore` / `--no-mempalignore` |
```

Remove P111 from the "当前 Spec" draft list only after code is complete.

- [ ] **Step 2: Mark the P111 plan complete**

Change the plan inventory line to:

```markdown
- `docs/plans/2026-07-06-p111-project-ignore-rules.md` — P111 project ignore rules（已完成）
```

- [ ] **Step 3: Run contract checks**

Run:

```bash
agent-spec parse specs/p111-project-ignore-rules.spec.md
agent-spec lint specs/p111-project-ignore-rules.spec.md --min-score 0.7
```

Expected: parse reports nonzero scenarios and lint score is at least 0.7.

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test --lib path_filter
cargo test --bin mempal test_init_detect_rooms_respects_gitignore_and_mempalignore
cargo test --bin mempal test_init_no_gitignore_keeps_mempalignore
cargo test --bin mempal test_init_custom_ignore_file_excludes_rooms
cargo test --bin mempal test_init_missing_ignore_file_fails
cargo test --bin mempal test_cli_init_ignore_flags_parse
cargo test --bin mempal test_cli_ingest_ignore_flags_parse
cargo test --test ingest_lock test_ingest_dir_respects_gitignore_and_mempalignore
cargo test --test ingest_lock test_ingest_no_mempalignore_keeps_gitignore
cargo test --test ingest_lock test_ingest_custom_ignore_file_excludes_sources
cargo test --test ingest_lock test_ingest_explicit_file_bypasses_project_ignore
```

Expected: all targeted tests pass.

- [ ] **Step 5: Run repo verification**

Run:

```bash
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test
```

Expected: all pass.

## Rollback

Revert the P111 commit. No database rollback is required because P111 does not
write schema changes or rewrite stored drawers. After rollback, future `init`
and directory `ingest` runs return to the old hard-coded traversal skips.
