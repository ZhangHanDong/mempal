use std::path::PathBuf;

use thiserror::Error;

use crate::core::{db::Database, types::ReindexSource};
use crate::embed::Embedder;

use super::{
    IngestError, IngestOptions, ingest_file_with_options, normalize::CURRENT_NORMALIZE_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexMode {
    Stale,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReindexOptions {
    pub mode: ReindexMode,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReindexReport {
    pub candidate_drawers: u64,
    pub candidate_sources: u64,
    pub processed_sources: u64,
    pub reingested_files: usize,
    pub reingested_chunks: usize,
    pub skipped_existing_chunks: usize,
    pub skipped_missing_sources: u64,
    pub skipped_missing_drawers: u64,
}

#[derive(Debug, Error)]
pub enum ReindexError {
    #[error(transparent)]
    Db(#[from] crate::core::db::DbError),
    #[error("failed to reindex source {source_file}")]
    Ingest {
        source_file: String,
        #[source]
        source: IngestError,
    },
}

pub async fn reindex_sources<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    options: ReindexOptions,
) -> Result<ReindexReport, ReindexError> {
    let sources = match options.mode {
        ReindexMode::Stale => db.reindex_sources_stale(CURRENT_NORMALIZE_VERSION)?,
        ReindexMode::Force => db.reindex_sources_force()?,
    };

    let mut report = ReindexReport {
        candidate_drawers: sources.iter().map(|source| source.drawer_count).sum(),
        candidate_sources: sources.len() as u64,
        ..ReindexReport::default()
    };

    // P117: the source-file existence scan runs in dry-run too, so the
    // report is a real feasibility statement (sources ingested without an
    // on-disk file — e.g. MCP-ingested content — are not reindexable).
    let mut reindexable = Vec::new();
    for source in sources {
        let has_file = source
            .source_file
            .as_deref()
            .is_some_and(|source_file| PathBuf::from(source_file).is_file());
        if has_file {
            reindexable.push(source);
        } else {
            report.skipped_missing_sources += 1;
            report.skipped_missing_drawers += source.drawer_count;
        }
    }

    if options.dry_run {
        return Ok(report);
    }

    for source in reindexable {
        let source_file = source
            .source_file
            .as_deref()
            .expect("checked Some above")
            .to_string();
        let source_path = PathBuf::from(&source_file);
        let stats = reindex_one_source(db, embedder, &source, &source_file, source_path).await?;
        report.processed_sources += 1;
        report.reingested_files += stats.files;
        report.reingested_chunks += stats.chunks;
        report.skipped_existing_chunks += stats.skipped;
    }

    Ok(report)
}

async fn reindex_one_source<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    source: &ReindexSource,
    source_file: &str,
    source_path: PathBuf,
) -> Result<super::IngestStats, ReindexError> {
    ingest_file_with_options(
        db,
        embedder,
        &source_path,
        &source.wing,
        IngestOptions {
            room: source.room.as_deref(),
            source_root: source_path.parent(),
            dry_run: false,
            source_file_override: Some(source_file),
            replace_existing_source: true,
            replace_across_rooms: true,
            no_strip_noise: false,
            ..IngestOptions::default()
        },
    )
    .await
    .map_err(|source| ReindexError::Ingest {
        source_file: source_file.to_string(),
        source,
    })
}
