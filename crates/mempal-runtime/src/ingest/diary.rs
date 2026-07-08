use crate::core::{
    db::Database,
    types::{AnchorKind, Drawer, MemoryDomain, Provenance, SourceType},
    utils::current_timestamp,
};
use crate::embed::Embedder;
use crate::ingest::lock;
use crate::ingest::normalize::CURRENT_NORMALIZE_VERSION;
use crate::ingest::{IngestError, IngestStats, LOCK_TIMEOUT, mempal_home_from_db};

pub const DIARY_ROLLUP_WING: &str = "agent-diary";
pub const DAILY_ROLLUP_LIMIT_BYTES: usize = 32 * 1024;
const DIARY_ROLLUP_SEPARATOR: &str = "\n\n---\n\n";

#[derive(Debug, Clone, Copy)]
pub struct DiaryRollupOptions<'a> {
    pub room: Option<&'a str>,
    pub day: Option<&'a str>,
    pub dry_run: bool,
    pub importance: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryRollupOutcome {
    pub drawer_id: String,
    pub stats: IngestStats,
}

pub struct PreparedDiaryRollup {
    pub drawer_id: String,
    pub content: String,
    pub room: String,
    pub day: String,
    pub stats: IngestStats,
    pub importance: i32,
    _lock_guard: lock::IngestLock,
}

pub fn current_rollup_day_utc() -> String {
    let days_since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn diary_rollup_drawer_id(room: &str, day: &str) -> String {
    format!("drawer_agent-diary_{room}_day_{day}")
}

pub fn prepare_diary_rollup(
    db: &Database,
    content: &str,
    wing: &str,
    options: DiaryRollupOptions<'_>,
) -> Result<PreparedDiaryRollup, IngestError> {
    if wing != DIARY_ROLLUP_WING {
        return Err(IngestError::DiaryRollupWrongWing {
            wing: wing.to_string(),
        });
    }

    let room = options
        .room
        .filter(|room| !room.trim().is_empty())
        .ok_or(IngestError::DiaryRollupMissingRoom)?;
    let day = options
        .day
        .map(ToOwned::to_owned)
        .unwrap_or_else(current_rollup_day_utc);
    let drawer_id = diary_rollup_drawer_id(room, &day);
    let mut stats = IngestStats {
        files: 1,
        chunks: 1,
        ..IngestStats::default()
    };

    let home = mempal_home_from_db(db);
    let lock_key = format!("diary_rollup_{room}_{day}");
    let lock_guard = lock::acquire_source_lock(&home, &lock_key, LOCK_TIMEOUT)?;
    stats.lock_wait_ms = Some(lock_guard.wait_duration().as_millis() as u64);

    let new_content =
        match db
            .get_drawer(&drawer_id)
            .map_err(|source| IngestError::CheckDrawer {
                drawer_id: drawer_id.clone(),
                source,
            })? {
            Some(existing) => format!("{}{DIARY_ROLLUP_SEPARATOR}{}", existing.content, content),
            None => content.to_string(),
        };

    if new_content.len() > DAILY_ROLLUP_LIMIT_BYTES {
        return Err(IngestError::DailyRollupFull {
            drawer_id,
            limit_bytes: DAILY_ROLLUP_LIMIT_BYTES,
            attempted_bytes: new_content.len(),
        });
    }

    Ok(PreparedDiaryRollup {
        drawer_id,
        content: new_content,
        room: room.to_string(),
        day,
        stats,
        importance: options.importance,
        _lock_guard: lock_guard,
    })
}

pub fn commit_prepared_diary_rollup(
    db: &Database,
    prepared: PreparedDiaryRollup,
    vector: &[f32],
) -> Result<DiaryRollupOutcome, IngestError> {
    let drawer = Drawer {
        id: prepared.drawer_id.clone(),
        content: prepared.content,
        wing: DIARY_ROLLUP_WING.to_string(),
        room: Some(prepared.room.clone()),
        source_file: Some(format!(
            "agent-diary://rollup/{}/{}",
            prepared.room, prepared.day
        )),
        source_type: SourceType::Manual,
        added_at: current_timestamp(),
        chunk_index: Some(0),
        normalize_version: CURRENT_NORMALIZE_VERSION,
        importance: prepared.importance,
        memory_kind: crate::core::types::MemoryKind::Evidence,
        domain: MemoryDomain::Agent,
        field: "diary".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: "repo://agent-diary".to_string(),
        parent_anchor_id: None,
        provenance: Some(Provenance::Runtime),
        statement: None,
        tier: None,
        status: None,
        supporting_refs: Vec::new(),
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    };

    db.upsert_drawer_and_replace_vector(&drawer, vector)
        .map_err(|source| IngestError::InsertDrawer {
            drawer_id: prepared.drawer_id.clone(),
            source,
        })?;

    Ok(DiaryRollupOutcome {
        drawer_id: prepared.drawer_id,
        stats: prepared.stats,
    })
}

pub async fn ingest_diary_rollup<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    content: &str,
    wing: &str,
    options: DiaryRollupOptions<'_>,
) -> Result<DiaryRollupOutcome, IngestError> {
    if options.dry_run {
        if wing != DIARY_ROLLUP_WING {
            return Err(IngestError::DiaryRollupWrongWing {
                wing: wing.to_string(),
            });
        }
        let room = options
            .room
            .filter(|room| !room.trim().is_empty())
            .ok_or(IngestError::DiaryRollupMissingRoom)?;
        let day = options
            .day
            .map(ToOwned::to_owned)
            .unwrap_or_else(current_rollup_day_utc);
        let drawer_id = diary_rollup_drawer_id(room, &day);
        let stats = IngestStats {
            files: 1,
            chunks: 1,
            ..IngestStats::default()
        };
        return Ok(DiaryRollupOutcome { drawer_id, stats });
    }

    let prepared = prepare_diary_rollup(db, content, wing, options)?;

    let vector = embedder
        .embed(&[prepared.content.as_str()])
        .await
        .map_err(|source| IngestError::EmbedChunks {
            path: std::path::PathBuf::from(format!(
                "agent-diary://rollup/{}/{}",
                prepared.room, prepared.day
            )),
            source,
        })?
        .into_iter()
        .next()
        .ok_or_else(|| IngestError::EmbedderReturnedNoVector {
            drawer_id: prepared.drawer_id.clone(),
        })?;

    commit_prepared_diary_rollup(db, prepared, &vector)
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}
