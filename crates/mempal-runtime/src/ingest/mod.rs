#![warn(clippy::all)]

pub mod chunk;
pub mod detect;
pub mod diary;
pub mod lock;
pub mod noise;
pub mod normalize;
pub mod reindex;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::core::{
    db::Database,
    types::{BootstrapEvidenceArgs, Drawer, SourceType},
    utils::{build_bootstrap_evidence_drawer_id, current_timestamp, route_room_from_taxonomy},
};
use crate::embed::{EmbedError, Embedder};
use crate::path_filter::{ProjectPathFilterOptions, project_walk};
use thiserror::Error;

use crate::ingest::{
    chunk::{chunk_conversation, chunk_text},
    detect::{Format, detect_format},
    normalize::{
        CURRENT_NORMALIZE_VERSION, NormalizeError, NormalizeOptions, normalize_content_with_options,
    },
};

const CHUNK_WINDOW: usize = 800;
const CHUNK_OVERLAP: usize = 100;

/// Max wait for per-source ingest lock before returning LockError::Timeout.
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Derive `mempal_home` from the DB path by taking the parent of
/// `palace.db`. Falls back to `./` on unusual layouts.
fn mempal_home_from_db(db: &Database) -> PathBuf {
    db.path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestStats {
    pub files: usize,
    pub chunks: usize,
    pub skipped: usize,
    pub noise_bytes_stripped: Option<u64>,
    /// Time waited acquiring the per-source ingest lock (P9-B). `None`
    /// when the lock was bypassed (e.g. dry-run) or when no wait was
    /// needed and the path took the fast exit before lock acquisition.
    pub lock_wait_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct IngestOptions<'a> {
    pub room: Option<&'a str>,
    pub source_root: Option<&'a Path>,
    pub dry_run: bool,
    pub source_file_override: Option<&'a str>,
    pub replace_existing_source: bool,
    /// When replacing an existing source, delete its prior drawers across all
    /// rooms (not just the freshly resolved room). Reindex sets this so a
    /// source that re-routes to a new room does not leave stale drawers behind
    /// in its old room. Ignored unless `replace_existing_source` is true.
    pub replace_across_rooms: bool,
    pub no_strip_noise: bool,
    pub diary_rollup: bool,
    pub diary_rollup_day: Option<&'a str>,
    pub project_filter: ProjectPathFilterOptions,
}

pub type Result<T> = std::result::Result<T, IngestError>;

#[derive(Debug, Error)]
pub enum IngestError {
    /// Transaction bookkeeping error from the store layer (P117).
    #[error(transparent)]
    Db(#[from] crate::core::db::DbError),
    #[error("failed to read {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to normalize {path}")]
    Normalize {
        path: PathBuf,
        #[source]
        source: NormalizeError,
    },
    #[error("failed to load taxonomy for wing {wing}")]
    LoadTaxonomy {
        wing: String,
        #[source]
        source: crate::core::db::DbError,
    },
    #[error("failed to embed chunks from {path}")]
    EmbedChunks {
        path: PathBuf,
        #[source]
        source: EmbedError,
    },
    #[error("failed to check drawer {drawer_id}")]
    CheckDrawer {
        drawer_id: String,
        #[source]
        source: crate::core::db::DbError,
    },
    #[error("failed to insert drawer {drawer_id}")]
    InsertDrawer {
        drawer_id: String,
        #[source]
        source: crate::core::db::DbError,
    },
    #[error("failed to replace source drawers for {source_file}")]
    ReplaceSource {
        source_file: String,
        #[source]
        source: crate::core::db::DbError,
    },
    #[error("failed to insert vector for {drawer_id}")]
    InsertVector {
        drawer_id: String,
        #[source]
        source: crate::core::db::DbError,
    },
    #[error("diary_rollup requires wing=\"agent-diary\", got wing=\"{wing}\"")]
    DiaryRollupWrongWing { wing: String },
    #[error("diary_rollup requires an explicit non-empty room")]
    DiaryRollupMissingRoom,
    #[error(
        "daily rollup drawer {drawer_id} would exceed {limit_bytes} bytes ({attempted_bytes} bytes)"
    )]
    DailyRollupFull {
        drawer_id: String,
        limit_bytes: usize,
        attempted_bytes: usize,
    },
    #[error("embedder returned no vector for {drawer_id}")]
    EmbedderReturnedNoVector { drawer_id: String },
    #[error("failed to acquire ingest lock: {0}")]
    Lock(#[from] lock::LockError),
    #[error("failed to apply project ignore rules: {0}")]
    ProjectFilter(#[from] crate::path_filter::ProjectPathFilterError),
    #[error("failed to walk project path {path}")]
    WalkPath {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },
    #[error("failed to read directory {path}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read entry in {path}")]
    ReadDirEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub async fn ingest_file<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    path: &Path,
    wing: &str,
    room: Option<&str>,
) -> Result<IngestStats> {
    ingest_file_with_options(
        db,
        embedder,
        path,
        wing,
        IngestOptions {
            room,
            source_root: path.parent(),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            replace_across_rooms: false,
            no_strip_noise: false,
            diary_rollup: false,
            diary_rollup_day: None,
            project_filter: ProjectPathFilterOptions::default(),
        },
    )
    .await
}

pub async fn ingest_file_with_options<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    path: &Path,
    wing: &str,
    options: IngestOptions<'_>,
) -> Result<IngestStats> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|source| IngestError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
    let content = String::from_utf8_lossy(&bytes).to_string();
    if content.trim().is_empty() {
        return Ok(IngestStats {
            files: 1,
            ..IngestStats::default()
        });
    }

    let format = detect_format(&content);
    let normalize_output = normalize_content_with_options(
        &content,
        format,
        NormalizeOptions {
            strip_noise: !options.no_strip_noise,
        },
    )
    .map_err(|source| IngestError::Normalize {
        path: path.to_path_buf(),
        source,
    })?;
    let normalized = normalize_output.content;
    let noise_bytes_stripped = normalize_output.noise_bytes_stripped;

    if options.diary_rollup {
        let mut outcome = diary::ingest_diary_rollup(
            db,
            embedder,
            &normalized,
            wing,
            diary::DiaryRollupOptions {
                room: options.room,
                day: options.diary_rollup_day,
                dry_run: options.dry_run,
                importance: 0,
            },
        )
        .await?;
        outcome.stats.noise_bytes_stripped = noise_bytes_stripped;
        return Ok(outcome.stats);
    }

    let resolved_room = match options.room {
        Some(room) => room.to_string(),
        None => {
            let taxonomy = db
                .taxonomy_entries()
                .map_err(|source| IngestError::LoadTaxonomy {
                    wing: wing.to_string(),
                    source,
                })?;
            route_room_from_taxonomy(&normalized, wing, &taxonomy)
        }
    };
    let chunks = match format {
        Format::ClaudeJsonl | Format::ChatGptJson | Format::CodexJsonl | Format::SlackJson => {
            chunk_conversation(&normalized)
        }
        Format::PlainText => chunk_text(&normalized, CHUNK_WINDOW, CHUNK_OVERLAP),
    };
    if chunks.is_empty() {
        return Ok(IngestStats {
            files: 1,
            ..IngestStats::default()
        });
    }

    let mut stats = IngestStats {
        files: 1,
        noise_bytes_stripped,
        ..IngestStats::default()
    };
    let source_file = options
        .source_file_override
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalize_source_file(path, options.source_root));

    // Per-source ingest lock (P9-B). Guards dedup-check + insert critical
    // section against concurrent Claude↔Codex ingests of the same source.
    // Skip in dry-run — no writes happen there, so race is impossible.
    let _lock_guard = if options.dry_run {
        None
    } else {
        let home = mempal_home_from_db(db);
        let key = lock::source_key(Path::new(&source_file));
        let guard = lock::acquire_source_lock(&home, &key, LOCK_TIMEOUT)?;
        stats.lock_wait_ms = Some(guard.wait_duration().as_millis() as u64);
        Some(guard)
    };

    let source_type = source_type_for(format);
    // P117: the destructive source replacement is deferred until AFTER
    // embeddings exist, and then runs in the same transaction as the
    // inserts — an embed or insert failure must never leave the source's
    // old drawers deleted.
    let replacing = options.replace_existing_source && !options.dry_run;

    let mut pending = Vec::new();
    let mut seen_drawer_ids = HashSet::new();

    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let drawer_id = build_bootstrap_evidence_drawer_id(
            wing,
            Some(resolved_room.as_str()),
            chunk,
            &source_type,
            Some(source_file.as_str()),
        );
        if !seen_drawer_ids.insert(drawer_id.clone()) {
            stats.skipped += 1;
            continue;
        }
        // Under replacement the existence check is skipped: drawer ids are
        // source-aware (P110), so a colliding id can only belong to this
        // same source, whose rows are deleted in the replace transaction.
        if !replacing
            && db
                .drawer_exists(&drawer_id)
                .map_err(|source| IngestError::CheckDrawer {
                    drawer_id: drawer_id.clone(),
                    source,
                })?
        {
            stats.skipped += 1;
            continue;
        }

        if options.dry_run {
            stats.chunks += 1;
            continue;
        }

        pending.push((chunk_index, chunk, drawer_id));
    }

    if options.dry_run || pending.is_empty() {
        return Ok(stats);
    }

    let chunk_refs = pending
        .iter()
        .map(|(_, chunk, _)| chunk.as_ref())
        .collect::<Vec<_>>();
    let vectors = embedder
        .embed(&chunk_refs)
        .await
        .map_err(|source| IngestError::EmbedChunks {
            path: path.to_path_buf(),
            source,
        })?;

    let rows: Vec<(usize, &str, String, Vec<f32>)> = pending
        .into_iter()
        .zip(vectors)
        .map(|((chunk_index, chunk, drawer_id), vector)| {
            (chunk_index, chunk.as_ref(), drawer_id, vector)
        })
        .collect();

    let insert_rows =
        |db: &Database, stats: &mut IngestStats| -> std::result::Result<(), IngestError> {
            for (chunk_index, chunk, drawer_id, vector) in &rows {
                let drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
                    id: drawer_id.clone(),
                    content: (*chunk).to_string(),
                    wing: wing.to_string(),
                    room: Some(resolved_room.clone()),
                    source_file: Some(source_file.clone()),
                    source_type: source_type.clone(),
                    added_at: current_timestamp(),
                    chunk_index: Some(*chunk_index as i64),
                    importance: 0,
                });
                let drawer = Drawer {
                    normalize_version: CURRENT_NORMALIZE_VERSION,
                    ..drawer
                };

                let inserted =
                    db.insert_drawer(&drawer)
                        .map_err(|source| IngestError::InsertDrawer {
                            drawer_id: drawer.id.clone(),
                            source,
                        })?;
                if inserted {
                    db.insert_vector(drawer_id, vector).map_err(|source| {
                        IngestError::InsertVector {
                            drawer_id: drawer.id.clone(),
                            source,
                        }
                    })?;
                    stats.chunks += 1;
                } else {
                    stats.skipped += 1;
                }
            }
            Ok(())
        };

    if replacing {
        // Delete-old + insert-new commit or roll back together.
        let mut txn_stats = IngestStats::default();
        db.with_immediate_transaction(|db| -> std::result::Result<(), IngestError> {
            let replace_result = if options.replace_across_rooms {
                db.replace_active_source_drawers_across_rooms_in_txn(&source_file, wing)
            } else {
                db.replace_active_source_drawers_in_txn(
                    &source_file,
                    wing,
                    Some(resolved_room.as_str()),
                )
            };
            replace_result.map_err(|source| IngestError::ReplaceSource {
                source_file: source_file.clone(),
                source,
            })?;
            insert_rows(db, &mut txn_stats)
        })?;
        stats.chunks += txn_stats.chunks;
        stats.skipped += txn_stats.skipped;
    } else {
        let mut direct_stats = IngestStats::default();
        insert_rows(db, &mut direct_stats)?;
        stats.chunks += direct_stats.chunks;
        stats.skipped += direct_stats.skipped;
    }

    Ok(stats)
}

pub async fn ingest_dir<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    dir: &Path,
    wing: &str,
    room: Option<&str>,
) -> Result<IngestStats> {
    ingest_dir_with_options(
        db,
        embedder,
        dir,
        wing,
        IngestOptions {
            room,
            source_root: Some(dir),
            dry_run: false,
            source_file_override: None,
            replace_existing_source: false,
            replace_across_rooms: false,
            no_strip_noise: false,
            diary_rollup: false,
            diary_rollup_day: None,
            project_filter: ProjectPathFilterOptions::default(),
        },
    )
    .await
}

pub async fn ingest_dir_with_options<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    dir: &Path,
    wing: &str,
    options: IngestOptions<'_>,
) -> Result<IngestStats> {
    let mut stats = IngestStats::default();

    for entry in project_walk(dir, &options.project_filter)? {
        let entry = entry.map_err(|source| IngestError::WalkPath {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path == dir || path.is_dir() {
            continue;
        }

        if path.is_file() {
            if should_skip_file(path, &options.project_filter) {
                stats.skipped += 1;
                continue;
            }
            let file_stats =
                ingest_file_with_options(db, embedder, path, wing, options.clone()).await?;
            stats.files += file_stats.files;
            stats.chunks += file_stats.chunks;
            stats.skipped += file_stats.skipped;
            stats.noise_bytes_stripped =
                merge_optional_sum(stats.noise_bytes_stripped, file_stats.noise_bytes_stripped);
        }
    }

    Ok(stats)
}

fn merge_optional_sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn source_type_for(format: Format) -> SourceType {
    match format {
        Format::ClaudeJsonl | Format::ChatGptJson | Format::CodexJsonl | Format::SlackJson => {
            SourceType::Conversation
        }
        Format::PlainText => SourceType::Project,
    }
}

fn should_skip_file(path: &Path, filter_options: &ProjectPathFilterOptions) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if matches!(name, ".DS_Store" | ".gitignore" | ".mempalignore") || name.starts_with("._") {
        return true;
    }
    if is_custom_ignore_file(path, filter_options) {
        return true;
    }

    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "a" | "bmp"
                    | "class"
                    | "dll"
                    | "dylib"
                    | "exe"
                    | "gif"
                    | "ico"
                    | "jar"
                    | "jpeg"
                    | "jpg"
                    | "o"
                    | "pdf"
                    | "png"
                    | "so"
                    | "wasm"
                    | "webp"
                    | "zip"
            )
        })
        .unwrap_or(false)
}

fn is_custom_ignore_file(path: &Path, filter_options: &ProjectPathFilterOptions) -> bool {
    filter_options.custom_ignore_files.iter().any(|custom| {
        path == custom || path.canonicalize().ok().as_ref() == custom.canonicalize().ok().as_ref()
    })
}

fn normalize_source_file(path: &Path, source_root: Option<&Path>) -> String {
    let normalized = source_root
        .and_then(|root| path.strip_prefix(root).ok())
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| path.file_name().map(PathBuf::from))
        .unwrap_or_else(|| path.to_path_buf());

    normalized.to_string_lossy().replace('\\', "/")
}
