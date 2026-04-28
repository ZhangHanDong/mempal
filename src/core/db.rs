use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::Value;
use thiserror::Error;

use super::anchor;
use super::types::{
    AnchorKind, ChunkNeighbors, Drawer, ExplicitTunnel, KnowledgeCard, KnowledgeCardEvent,
    KnowledgeCardFilter, KnowledgeEventType, KnowledgeEvidenceLink, KnowledgeEvidenceRole,
    KnowledgeStatus, KnowledgeTier, MemoryDomain, MemoryKind, NeighborChunk, Provenance,
    ReindexSource, SourceType, TaxonomyEntry, Triple, TripleStats, TunnelEndpoint,
    TunnelFollowResult,
};
use super::utils::{build_tunnel_id, current_timestamp, format_tunnel_endpoint};

const CURRENT_SCHEMA_VERSION: u32 = 8;
const DRAWER_SELECT_COLUMNS: &str = r#"
    id,
    content,
    wing,
    room,
    source_file,
    source_type,
    added_at,
    chunk_index,
    normalize_version,
    COALESCE(importance, 0) as importance,
    memory_kind,
    domain,
    field,
    anchor_kind,
    anchor_id,
    parent_anchor_id,
    provenance,
    statement,
    tier,
    status,
    supporting_refs,
    counterexample_refs,
    teaching_refs,
    verification_refs,
    scope_constraints,
    trigger_hints
"#;

const V1_SCHEMA_SQL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS drawers (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    wing TEXT NOT NULL,
    room TEXT,
    source_file TEXT,
    source_type TEXT NOT NULL CHECK(source_type IN ('project', 'conversation', 'manual')),
    added_at TEXT NOT NULL,
    chunk_index INTEGER
);

-- drawer_vectors is created lazily by insert_vector() with the actual
-- embedding dimension from the configured embedder. This avoids hardcoding
-- a dimension that may not match the model in use.

CREATE TABLE IF NOT EXISTS triples (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    object TEXT NOT NULL,
    valid_from TEXT,
    valid_to TEXT,
    confidence REAL DEFAULT 1.0,
    source_drawer TEXT REFERENCES drawers(id)
);

CREATE TABLE IF NOT EXISTS taxonomy (
    wing TEXT NOT NULL,
    room TEXT NOT NULL DEFAULT '',
    display_name TEXT,
    keywords TEXT,
    PRIMARY KEY (wing, room)
);

CREATE INDEX IF NOT EXISTS idx_drawers_wing ON drawers(wing);
CREATE INDEX IF NOT EXISTS idx_drawers_wing_room ON drawers(wing, room);
CREATE INDEX IF NOT EXISTS idx_triples_subject ON triples(subject);
CREATE INDEX IF NOT EXISTS idx_triples_object ON triples(object);
"#;

static SQLITE_VEC_AUTO_EXTENSION: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to create database directory for {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read database metadata for {path}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("failed to parse taxonomy keywords JSON")]
    Json(#[from] serde_json::Error),
    #[error("invalid source_type stored in database: {0}")]
    InvalidSourceType(String),
    #[error("invalid {kind} stored in database: {value}")]
    InvalidEnumValue { kind: &'static str, value: String },
    #[error("invalid drawer metadata: {0}")]
    InvalidDrawerMetadata(String),
    #[error("invalid tunnel: {0}")]
    InvalidTunnel(String),
    #[error("failed to register sqlite-vec auto extension: {0}")]
    RegisterVec(String),
    #[error("database schema version {current} is newer than supported version {supported}")]
    UnsupportedSchemaVersion { current: u32, supported: u32 },
}

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| DbError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        register_sqlite_vec()?;

        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        apply_migrations(&conn)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn insert_drawer(&self, drawer: &Drawer) -> Result<(), DbError> {
        anchor::validate_anchor_domain(&drawer.domain, &drawer.anchor_kind)
            .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;

        self.conn.execute(
            r#"
            INSERT INTO drawers (
                id,
                content,
                wing,
                room,
                source_file,
                source_type,
                added_at,
                chunk_index,
                normalize_version,
                importance,
                memory_kind,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                provenance,
                statement,
                tier,
                status,
                supporting_refs,
                counterexample_refs,
                teaching_refs,
                verification_refs,
                scope_constraints,
                trigger_hints
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)
            "#,
            params![
                drawer.id.as_str(),
                drawer.content.as_str(),
                drawer.wing.as_str(),
                drawer.room.as_deref(),
                drawer.source_file.as_deref(),
                source_type_as_str(&drawer.source_type),
                drawer.added_at.as_str(),
                drawer.chunk_index,
                i64::from(drawer.normalize_version),
                drawer.importance,
                memory_kind_as_str(&drawer.memory_kind),
                memory_domain_as_str(&drawer.domain),
                drawer.field.as_str(),
                anchor_kind_as_str(&drawer.anchor_kind),
                drawer.anchor_id.as_str(),
                drawer.parent_anchor_id.as_deref(),
                drawer.provenance.as_ref().map(provenance_as_str),
                drawer.statement.as_deref(),
                drawer.tier.as_ref().map(knowledge_tier_as_str),
                drawer.status.as_ref().map(knowledge_status_as_str),
                encode_json(&drawer.supporting_refs)?,
                encode_json(&drawer.counterexample_refs)?,
                encode_json(&drawer.teaching_refs)?,
                encode_json(&drawer.verification_refs)?,
                drawer.scope_constraints.as_deref(),
                encode_optional_json(drawer.trigger_hints.as_ref())?,
            ],
        )?;

        Ok(())
    }

    pub fn taxonomy_entries(&self) -> Result<Vec<TaxonomyEntry>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT wing, room, display_name, keywords FROM taxonomy ORDER BY wing, room",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (wing, room, display_name, keywords_json) = row?;
            let keywords = parse_keywords(keywords_json.as_deref())?;
            entries.push(TaxonomyEntry {
                wing,
                room,
                display_name,
                keywords,
            });
        }

        Ok(entries)
    }

    pub fn upsert_taxonomy_entry(&self, entry: &TaxonomyEntry) -> Result<(), DbError> {
        let keywords = serde_json::to_string(&entry.keywords)?;
        self.conn.execute(
            r#"
            INSERT INTO taxonomy (wing, room, display_name, keywords)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(wing, room) DO UPDATE SET
                display_name = excluded.display_name,
                keywords = excluded.keywords
            "#,
            (
                entry.wing.as_str(),
                entry.room.as_str(),
                entry.display_name.as_deref(),
                keywords.as_str(),
            ),
        )?;

        Ok(())
    }

    /// Returns top drawers sorted by importance (descending), then recency.
    pub fn top_drawers(&self, limit: usize) -> Result<Vec<Drawer>, DbError> {
        let limit = i64::try_from(limit)
            .map_err(|_| rusqlite::Error::InvalidParameterName("limit".to_string()))?;
        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT {DRAWER_SELECT_COLUMNS}
            FROM drawers
            WHERE deleted_at IS NULL
            ORDER BY importance DESC, CAST(added_at AS INTEGER) DESC, id DESC
            LIMIT ?1
            "#,
        ))?;
        let rows = statement.query_map([limit], |row| {
            drawer_from_row(row).map_err(row_decode_error)
        })?;

        let mut drawers = Vec::new();
        for row in rows {
            drawers.push(row?);
        }

        Ok(drawers)
    }

    pub fn drawer_exists(&self, drawer_id: &str) -> Result<bool, DbError> {
        let exists = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM drawers WHERE id = ?1 AND deleted_at IS NULL)",
            [drawer_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(exists == 1)
    }

    pub fn insert_vector(&self, drawer_id: &str, vector: &[f32]) -> Result<(), DbError> {
        self.ensure_vectors_table(vector.len())?;
        let vector_json = serde_json::to_string(vector)?;
        self.conn.execute(
            "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
            (drawer_id, vector_json.as_str()),
        )?;
        Ok(())
    }

    pub fn upsert_drawer_and_replace_vector(
        &self,
        drawer: &Drawer,
        vector: &[f32],
    ) -> Result<(), DbError> {
        anchor::validate_anchor_domain(&drawer.domain, &drawer.anchor_kind)
            .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;
        self.ensure_vectors_table(vector.len())?;

        let existing = self
            .conn
            .query_row(
                "SELECT rowid, content FROM drawers WHERE id = ?1 AND deleted_at IS NULL",
                [drawer.id.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        if existing.is_none() {
            self.insert_drawer(drawer)?;
            return self.insert_vector(&drawer.id, vector);
        }

        let (rowid, old_content) = existing.expect("checked Some");
        let vector_json = serde_json::to_string(vector)?;

        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result = (|| -> Result<(), DbError> {
            if self.table_exists("drawers_fts")? {
                self.conn.execute(
                    "INSERT INTO drawers_fts(drawers_fts, rowid, content) VALUES ('delete', ?1, ?2)",
                    params![rowid, old_content],
                )?;
            }

            self.conn.execute(
                r#"
                UPDATE drawers
                SET content = ?2,
                    wing = ?3,
                    room = ?4,
                    source_file = ?5,
                    source_type = ?6,
                    added_at = ?7,
                    chunk_index = ?8,
                    normalize_version = ?9,
                    importance = ?10,
                    memory_kind = ?11,
                    domain = ?12,
                    field = ?13,
                    anchor_kind = ?14,
                    anchor_id = ?15,
                    parent_anchor_id = ?16,
                    provenance = ?17,
                    statement = ?18,
                    tier = ?19,
                    status = ?20,
                    supporting_refs = ?21,
                    counterexample_refs = ?22,
                    teaching_refs = ?23,
                    verification_refs = ?24,
                    scope_constraints = ?25,
                    trigger_hints = ?26
                WHERE id = ?1 AND deleted_at IS NULL
                "#,
                params![
                    drawer.id.as_str(),
                    drawer.content.as_str(),
                    drawer.wing.as_str(),
                    drawer.room.as_deref(),
                    drawer.source_file.as_deref(),
                    source_type_as_str(&drawer.source_type),
                    drawer.added_at.as_str(),
                    drawer.chunk_index,
                    i64::from(drawer.normalize_version),
                    drawer.importance,
                    memory_kind_as_str(&drawer.memory_kind),
                    memory_domain_as_str(&drawer.domain),
                    drawer.field.as_str(),
                    anchor_kind_as_str(&drawer.anchor_kind),
                    drawer.anchor_id.as_str(),
                    drawer.parent_anchor_id.as_deref(),
                    drawer.provenance.as_ref().map(provenance_as_str),
                    drawer.statement.as_deref(),
                    drawer.tier.as_ref().map(knowledge_tier_as_str),
                    drawer.status.as_ref().map(knowledge_status_as_str),
                    encode_json(&drawer.supporting_refs)?,
                    encode_json(&drawer.counterexample_refs)?,
                    encode_json(&drawer.teaching_refs)?,
                    encode_json(&drawer.verification_refs)?,
                    drawer.scope_constraints.as_deref(),
                    encode_optional_json(drawer.trigger_hints.as_ref())?,
                ],
            )?;

            if self.table_exists("drawers_fts")? {
                self.conn.execute(
                    "INSERT INTO drawers_fts(rowid, content) VALUES (?1, ?2)",
                    params![rowid, drawer.content.as_str()],
                )?;
            }

            self.conn.execute(
                "DELETE FROM drawer_vectors WHERE id = ?1",
                [drawer.id.as_str()],
            )?;
            self.conn.execute(
                "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
                params![drawer.id.as_str(), vector_json.as_str()],
            )?;

            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }

    /// Ensure drawer_vectors table exists with the right dimension.
    /// Creates it on first call; errors on dimension mismatch.
    fn ensure_vectors_table(&self, dim: usize) -> Result<(), DbError> {
        // Check if table exists
        let exists: bool = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='drawer_vectors')",
                [],
                |row| row.get(0),
            )?;

        if !exists {
            self.conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS drawer_vectors USING vec0(id TEXT PRIMARY KEY, embedding FLOAT[{dim}]);"
            ))?;
        }
        Ok(())
    }

    pub fn drawer_count(&self) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn stale_drawer_count(&self, current_normalize_version: u32) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NULL AND normalize_version < ?1",
            [i64::from(current_normalize_version)],
            |row| row.get(0),
        )?)
    }

    pub fn drawer_count_by_normalize_version(&self) -> Result<Vec<(u32, i64)>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT normalize_version, COUNT(*)
            FROM drawers
            WHERE deleted_at IS NULL
            GROUP BY normalize_version
            ORDER BY normalize_version
            "#,
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn diary_rollup_days(&self) -> Result<u32, DbError> {
        let count = self.conn.query_row(
            r#"
            SELECT COUNT(DISTINCT substr(source_file, length(source_file) - 9, 10))
            FROM drawers
            WHERE deleted_at IS NULL
              AND wing = 'agent-diary'
              AND source_file LIKE 'agent-diary://rollup/%'
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as u32)
    }

    pub fn reindex_sources_stale(
        &self,
        current_normalize_version: u32,
    ) -> Result<Vec<ReindexSource>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT source_file, wing, room, COUNT(*)
            FROM drawers
            WHERE deleted_at IS NULL AND normalize_version < ?1
            GROUP BY source_file, wing, room
            ORDER BY source_file, wing, room
            "#,
        )?;
        let rows = statement
            .query_map(
                [i64::from(current_normalize_version)],
                reindex_source_from_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn reindex_sources_force(&self) -> Result<Vec<ReindexSource>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT source_file, wing, room, COUNT(*)
            FROM drawers
            WHERE deleted_at IS NULL
            GROUP BY source_file, wing, room
            ORDER BY source_file, wing, room
            "#,
        )?;
        let rows = statement
            .query_map([], reindex_source_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn replace_active_source_drawers(
        &self,
        source_file: &str,
        wing: &str,
        room: Option<&str>,
    ) -> Result<u64, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT rowid, id, content
            FROM drawers
            WHERE deleted_at IS NULL
              AND source_file = ?1
              AND wing = ?2
              AND ((?3 IS NULL AND room IS NULL) OR room = ?3)
            ORDER BY rowid
            "#,
        )?;
        let rows = statement
            .query_map((source_file, wing, room), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);

        if rows.is_empty() {
            return Ok(0);
        }

        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result = (|| -> Result<u64, DbError> {
            let fts_exists = self.table_exists("drawers_fts")?;
            let vectors_exist = self.table_exists("drawer_vectors")?;

            for (rowid, id, content) in &rows {
                if fts_exists {
                    self.conn.execute(
                        "INSERT INTO drawers_fts(drawers_fts, rowid, content) VALUES ('delete', ?1, ?2)",
                        params![rowid, content],
                    )?;
                }
                if vectors_exist {
                    self.conn
                        .execute("DELETE FROM drawer_vectors WHERE id = ?1", [id])?;
                }
                self.conn
                    .execute("DELETE FROM drawers WHERE rowid = ?1", [rowid])?;
            }

            Ok(rows.len() as u64)
        })();

        match result {
            Ok(count) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(count)
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }

    fn table_exists(&self, table_name: &str) -> Result<bool, DbError> {
        let exists = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1)",
            [table_name],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(exists == 1)
    }

    pub fn taxonomy_count(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM taxonomy", [], |row| row.get(0))?)
    }

    pub fn scope_counts(&self) -> Result<Vec<(String, Option<String>, i64)>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT wing, room, COUNT(*)
            FROM drawers
            WHERE deleted_at IS NULL
            GROUP BY wing, room
            ORDER BY wing, room
            "#,
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_drawer(&self, drawer_id: &str) -> Result<Option<Drawer>, DbError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT {DRAWER_SELECT_COLUMNS}
            FROM drawers
            WHERE id = ?1 AND deleted_at IS NULL
            "#,
        ))?;
        let mut rows = statement.query_map([drawer_id], |row| {
            drawer_from_row(row).map_err(row_decode_error)
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_knowledge_lifecycle(
        &self,
        drawer_id: &str,
        status: &KnowledgeStatus,
        verification_refs: &[String],
        counterexample_refs: &[String],
    ) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            r#"
            UPDATE drawers
            SET status = ?2,
                verification_refs = ?3,
                counterexample_refs = ?4
            WHERE id = ?1
              AND deleted_at IS NULL
              AND memory_kind = 'knowledge'
            "#,
            params![
                drawer_id,
                knowledge_status_as_str(status),
                encode_json(verification_refs)?,
                encode_json(counterexample_refs)?,
            ],
        )?;
        Ok(affected > 0)
    }

    pub fn update_knowledge_anchor(
        &self,
        drawer_id: &str,
        anchor_kind: &AnchorKind,
        anchor_id: &str,
        parent_anchor_id: Option<&str>,
    ) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            r#"
            UPDATE drawers
            SET anchor_kind = ?2,
                anchor_id = ?3,
                parent_anchor_id = ?4
            WHERE id = ?1
              AND deleted_at IS NULL
              AND memory_kind = 'knowledge'
            "#,
            params![
                drawer_id,
                anchor_kind_as_str(anchor_kind),
                anchor_id,
                parent_anchor_id,
            ],
        )?;
        Ok(affected > 0)
    }

    pub fn insert_knowledge_card(&self, card: &KnowledgeCard) -> Result<(), DbError> {
        anchor::validate_anchor_domain(&card.domain, &card.anchor_kind)
            .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;

        self.conn.execute(
            r#"
            INSERT INTO knowledge_cards (
                id,
                statement,
                content,
                tier,
                status,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                scope_constraints,
                trigger_hints,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                card.id.as_str(),
                card.statement.as_str(),
                card.content.as_str(),
                knowledge_tier_as_str(&card.tier),
                knowledge_status_as_str(&card.status),
                memory_domain_as_str(&card.domain),
                card.field.as_str(),
                anchor_kind_as_str(&card.anchor_kind),
                card.anchor_id.as_str(),
                card.parent_anchor_id.as_deref(),
                card.scope_constraints.as_deref(),
                encode_optional_json(card.trigger_hints.as_ref())?,
                card.created_at.as_str(),
                card.updated_at.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn get_knowledge_card(&self, card_id: &str) -> Result<Option<KnowledgeCard>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT
                id,
                statement,
                content,
                tier,
                status,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                scope_constraints,
                trigger_hints,
                created_at,
                updated_at
            FROM knowledge_cards
            WHERE id = ?1
            "#,
        )?;
        let mut rows = statement.query_map([card_id], |row| {
            knowledge_card_from_row(row).map_err(row_decode_error)
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_knowledge_cards(
        &self,
        filter: &KnowledgeCardFilter,
    ) -> Result<Vec<KnowledgeCard>, DbError> {
        let tier = filter.tier.as_ref().map(knowledge_tier_as_str);
        let status = filter.status.as_ref().map(knowledge_status_as_str);
        let domain = filter.domain.as_ref().map(memory_domain_as_str);
        let anchor_kind = filter.anchor_kind.as_ref().map(anchor_kind_as_str);

        let mut statement = self.conn.prepare(
            r#"
            SELECT
                id,
                statement,
                content,
                tier,
                status,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                scope_constraints,
                trigger_hints,
                created_at,
                updated_at
            FROM knowledge_cards
            WHERE (?1 IS NULL OR tier = ?1)
              AND (?2 IS NULL OR status = ?2)
              AND (?3 IS NULL OR domain = ?3)
              AND (?4 IS NULL OR field = ?4)
              AND (?5 IS NULL OR anchor_kind = ?5)
              AND (?6 IS NULL OR anchor_id = ?6)
            ORDER BY tier, status, id
            "#,
        )?;
        let rows = statement
            .query_map(
                params![
                    tier,
                    status,
                    domain,
                    filter.field.as_deref(),
                    anchor_kind,
                    filter.anchor_id.as_deref(),
                ],
                |row| knowledge_card_from_row(row).map_err(row_decode_error),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn knowledge_card_count(&self) -> Result<i64, DbError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM knowledge_cards", [], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn list_knowledge_drawers_for_card_backfill(
        &self,
        filter: &KnowledgeCardFilter,
    ) -> Result<Vec<Drawer>, DbError> {
        let tier = filter.tier.as_ref().map(knowledge_tier_as_str);
        let status = filter.status.as_ref().map(knowledge_status_as_str);
        let domain = filter.domain.as_ref().map(memory_domain_as_str);
        let anchor_kind = filter.anchor_kind.as_ref().map(anchor_kind_as_str);

        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT {DRAWER_SELECT_COLUMNS}
            FROM drawers
            WHERE deleted_at IS NULL
              AND memory_kind = 'knowledge'
              AND (?1 IS NULL OR tier = ?1)
              AND (?2 IS NULL OR status = ?2)
              AND (?3 IS NULL OR domain = ?3)
              AND (?4 IS NULL OR field = ?4)
              AND (?5 IS NULL OR anchor_kind = ?5)
              AND (?6 IS NULL OR anchor_id = ?6)
            ORDER BY id
            "#,
        ))?;
        let rows = statement
            .query_map(
                params![
                    tier,
                    status,
                    domain,
                    filter.field.as_deref(),
                    anchor_kind,
                    filter.anchor_id.as_deref(),
                ],
                |row| drawer_from_row(row).map_err(row_decode_error),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_knowledge_card(&self, card: &KnowledgeCard) -> Result<bool, DbError> {
        anchor::validate_anchor_domain(&card.domain, &card.anchor_kind)
            .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;

        let affected = self.conn.execute(
            r#"
            UPDATE knowledge_cards
            SET statement = ?2,
                content = ?3,
                tier = ?4,
                status = ?5,
                domain = ?6,
                field = ?7,
                anchor_kind = ?8,
                anchor_id = ?9,
                parent_anchor_id = ?10,
                scope_constraints = ?11,
                trigger_hints = ?12,
                updated_at = ?13
            WHERE id = ?1
            "#,
            params![
                card.id.as_str(),
                card.statement.as_str(),
                card.content.as_str(),
                knowledge_tier_as_str(&card.tier),
                knowledge_status_as_str(&card.status),
                memory_domain_as_str(&card.domain),
                card.field.as_str(),
                anchor_kind_as_str(&card.anchor_kind),
                card.anchor_id.as_str(),
                card.parent_anchor_id.as_deref(),
                card.scope_constraints.as_deref(),
                encode_optional_json(card.trigger_hints.as_ref())?,
                card.updated_at.as_str(),
            ],
        )?;
        Ok(affected > 0)
    }

    pub fn insert_knowledge_evidence_link(
        &self,
        link: &KnowledgeEvidenceLink,
    ) -> Result<(), DbError> {
        let evidence = self.get_drawer(&link.evidence_drawer_id)?.ok_or_else(|| {
            DbError::InvalidDrawerMetadata(format!(
                "evidence drawer {} does not exist",
                link.evidence_drawer_id
            ))
        })?;
        if evidence.memory_kind != MemoryKind::Evidence {
            return Err(DbError::InvalidDrawerMetadata(format!(
                "evidence link target {} must be an evidence drawer",
                link.evidence_drawer_id
            )));
        }

        self.conn.execute(
            r#"
            INSERT INTO knowledge_evidence_links (
                id,
                card_id,
                evidence_drawer_id,
                role,
                note,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                link.id.as_str(),
                link.card_id.as_str(),
                link.evidence_drawer_id.as_str(),
                knowledge_evidence_role_as_str(&link.role),
                link.note.as_deref(),
                link.created_at.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn knowledge_evidence_links(
        &self,
        card_id: &str,
    ) -> Result<Vec<KnowledgeEvidenceLink>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, card_id, evidence_drawer_id, role, note, created_at
            FROM knowledge_evidence_links
            WHERE card_id = ?1
            ORDER BY created_at, id
            "#,
        )?;
        let rows = statement
            .query_map([card_id], |row| {
                knowledge_evidence_link_from_row(row).map_err(row_decode_error)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn knowledge_evidence_links_for_drawer(
        &self,
        evidence_drawer_id: &str,
    ) -> Result<Vec<KnowledgeEvidenceLink>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, card_id, evidence_drawer_id, role, note, created_at
            FROM knowledge_evidence_links
            WHERE evidence_drawer_id = ?1
            ORDER BY created_at, id
            "#,
        )?;
        let rows = statement
            .query_map([evidence_drawer_id], |row| {
                knowledge_evidence_link_from_row(row).map_err(row_decode_error)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn append_knowledge_event(&self, event: &KnowledgeCardEvent) -> Result<(), DbError> {
        self.conn.execute(
            r#"
            INSERT INTO knowledge_events (
                id,
                card_id,
                event_type,
                from_status,
                to_status,
                reason,
                actor,
                metadata,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                event.id.as_str(),
                event.card_id.as_str(),
                knowledge_event_type_as_str(&event.event_type),
                event.from_status.as_ref().map(knowledge_status_as_str),
                event.to_status.as_ref().map(knowledge_status_as_str),
                event.reason.as_str(),
                event.actor.as_deref(),
                encode_optional_json(event.metadata.as_ref())?,
                event.created_at.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn knowledge_events(&self, card_id: &str) -> Result<Vec<KnowledgeCardEvent>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT
                id,
                card_id,
                event_type,
                from_status,
                to_status,
                reason,
                actor,
                metadata,
                created_at
            FROM knowledge_events
            WHERE card_id = ?1
            ORDER BY created_at, id
            "#,
        )?;
        let rows = statement
            .query_map([card_id], |row| {
                knowledge_event_from_row(row).map_err(row_decode_error)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn neighbor_chunks(
        &self,
        source_file: &str,
        wing: &str,
        room: Option<&str>,
        chunk_index: i64,
    ) -> Result<ChunkNeighbors, DbError> {
        let prev_index = chunk_index - 1;
        let next_index = chunk_index + 1;
        let sql = r#"
            SELECT id, content, chunk_index
            FROM drawers
            WHERE deleted_at IS NULL
              AND source_file = ?1
              AND wing = ?2
              AND ((?3 IS NULL AND room IS NULL) OR (?3 IS NOT NULL AND room = ?3))
              AND chunk_index IN (?4, ?5)
            ORDER BY chunk_index, id
            "#;
        let mut statement = self.conn.prepare(sql)?;
        let mut rows = statement.query(params![source_file, wing, room, prev_index, next_index])?;
        let mut neighbors = ChunkNeighbors {
            prev: None,
            next: None,
        };

        while let Some(row) = rows.next()? {
            let row_index = row.get::<_, i64>(2)?;
            let Ok(chunk_index) = u32::try_from(row_index) else {
                continue;
            };
            let chunk = NeighborChunk {
                drawer_id: row.get(0)?,
                content: row.get(1)?,
                chunk_index,
            };
            if row_index == prev_index && neighbors.prev.is_none() {
                neighbors.prev = Some(chunk);
            } else if row_index == next_index && neighbors.next.is_none() {
                neighbors.next = Some(chunk);
            }
        }

        Ok(neighbors)
    }

    pub fn soft_delete_drawer(&self, drawer_id: &str) -> Result<bool, DbError> {
        let timestamp = super::utils::current_timestamp();
        let affected = self.conn.execute(
            "UPDATE drawers SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![timestamp, drawer_id],
        )?;
        Ok(affected > 0)
    }

    pub fn purge_deleted(&self, before: Option<&str>) -> Result<u64, DbError> {
        // First collect IDs to purge, then delete from both tables
        let ids: Vec<String> = if let Some(before) = before {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM drawers WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
            )?;
            stmt.query_map([before], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM drawers WHERE deleted_at IS NOT NULL")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        if ids.is_empty() {
            return Ok(0);
        }

        // Check if drawer_vectors table exists (lazy-created)
        let vectors_exist: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='drawer_vectors')",
            [],
            |row| row.get(0),
        )?;

        for id in &ids {
            if vectors_exist {
                self.conn
                    .execute("DELETE FROM drawer_vectors WHERE id = ?1", [id])?;
            }
            self.conn
                .execute("DELETE FROM drawers WHERE id = ?1", [id])?;
        }

        Ok(ids.len() as u64)
    }

    pub fn deleted_drawer_count(&self) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?)
    }

    // --- FTS5 BM25 search ---

    pub fn search_fts(
        &self,
        query: &str,
        wing: Option<&str>,
        room: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, DbError> {
        let Some(match_query) = build_fts_match_query(query) else {
            return Ok(Vec::new());
        };
        let limit =
            i64::try_from(limit).map_err(|_| DbError::InvalidSourceType("limit".to_string()))?;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT d.id, fts.rank
            FROM drawers_fts fts
            JOIN drawers d ON d.rowid = fts.rowid
            WHERE drawers_fts MATCH ?1
              AND d.deleted_at IS NULL
              AND (?2 IS NULL OR d.wing = ?2)
              AND (?3 IS NULL OR d.room = ?3)
            ORDER BY fts.rank
            LIMIT ?4
            "#,
        )?;
        let rows = stmt
            .query_map((match_query.as_str(), wing, room, limit), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --- Triples (Knowledge Graph) ---

    pub fn insert_triple(&self, triple: &Triple) -> Result<(), DbError> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO triples (id, subject, predicate, object, valid_from, valid_to, confidence, source_drawer)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                triple.id,
                triple.subject,
                triple.predicate,
                triple.object,
                triple.valid_from,
                triple.valid_to,
                triple.confidence,
                triple.source_drawer,
            ],
        )?;
        Ok(())
    }

    pub fn query_triples(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<Triple>, DbError> {
        let active_clause = if active_only {
            "AND (valid_to IS NULL OR valid_to > strftime('%s', 'now'))"
        } else {
            ""
        };
        let sql = format!(
            r#"
            SELECT id, subject, predicate, object, valid_from, valid_to, confidence, source_drawer
            FROM triples
            WHERE (?1 IS NULL OR subject = ?1)
              AND (?2 IS NULL OR predicate = ?2)
              AND (?3 IS NULL OR object = ?3)
              {active_clause}
            ORDER BY confidence DESC, id
            "#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map((subject, predicate, object), |row| {
                Ok(Triple {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    predicate: row.get(2)?,
                    object: row.get(3)?,
                    valid_from: row.get(4)?,
                    valid_to: row.get(5)?,
                    confidence: row.get(6)?,
                    source_drawer: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn invalidate_triple(&self, triple_id: &str) -> Result<bool, DbError> {
        let timestamp = super::utils::current_timestamp();
        let affected = self.conn.execute(
            "UPDATE triples SET valid_to = ?1 WHERE id = ?2 AND valid_to IS NULL",
            params![timestamp, triple_id],
        )?;
        Ok(affected > 0)
    }

    pub fn triple_count(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM triples", [], |row| row.get(0))?)
    }

    pub fn timeline_for_entity(&self, entity: &str) -> Result<Vec<Triple>, DbError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, subject, predicate, object, valid_from, valid_to, confidence, source_drawer
            FROM triples
            WHERE subject = ?1 OR object = ?1
            ORDER BY COALESCE(valid_from, '0') ASC, id ASC
            "#,
        )?;
        let rows = stmt
            .query_map([entity], |row| {
                Ok(Triple {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    predicate: row.get(2)?,
                    object: row.get(3)?,
                    valid_from: row.get(4)?,
                    valid_to: row.get(5)?,
                    confidence: row.get(6)?,
                    source_drawer: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn triple_stats(&self) -> Result<TripleStats, DbError> {
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM triples", [], |row| row.get(0))?;
        let active: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM triples WHERE valid_to IS NULL",
            [],
            |row| row.get(0),
        )?;
        let expired = total - active;
        let entities: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(DISTINCT entity) FROM (
                SELECT subject AS entity FROM triples
                UNION
                SELECT object AS entity FROM triples
            )
            "#,
            [],
            |row| row.get(0),
        )?;
        let mut top_predicates_stmt = self.conn.prepare(
            "SELECT predicate, COUNT(*) as cnt FROM triples GROUP BY predicate ORDER BY cnt DESC LIMIT 5",
        )?;
        let top_predicates = top_predicates_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(TripleStats {
            total,
            active,
            expired,
            entities,
            top_predicates,
        })
    }

    // --- Tunnels (cross-Wing discovery) ---

    pub fn find_tunnels(&self) -> Result<Vec<(String, Vec<String>)>, DbError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT room, GROUP_CONCAT(DISTINCT wing) as wings
            FROM drawers
            WHERE deleted_at IS NULL AND room IS NOT NULL AND room != ''
            GROUP BY room
            HAVING COUNT(DISTINCT wing) > 1
            ORDER BY room
            "#,
        )?;
        let rows = stmt
            .query_map([], |row| {
                let room: String = row.get(0)?;
                let wings_csv: String = row.get(1)?;
                Ok((room, wings_csv))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(room, wings_csv)| {
                let wings = wings_csv.split(',').map(ToOwned::to_owned).collect();
                (room, wings)
            })
            .collect())
    }

    pub fn create_tunnel(
        &self,
        left: &TunnelEndpoint,
        right: &TunnelEndpoint,
        label: &str,
        created_by: Option<&str>,
    ) -> Result<ExplicitTunnel, DbError> {
        let left = normalize_tunnel_endpoint(left)?;
        let right = normalize_tunnel_endpoint(right)?;
        let label = label.trim();
        if label.is_empty() {
            return Err(DbError::InvalidTunnel("label is required".to_string()));
        }
        if left == right {
            return Err(DbError::InvalidTunnel(
                "self-link is not allowed".to_string(),
            ));
        }

        let id = build_tunnel_id(&left, &right);
        let created_at = current_timestamp();
        let created_by = created_by
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        self.conn.execute(
            r#"
            INSERT INTO tunnels (
                id, left_wing, left_room, right_wing, right_room,
                label, created_at, created_by, deleted_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            ON CONFLICT(id) DO UPDATE SET
                label = CASE
                    WHEN tunnels.deleted_at IS NOT NULL THEN excluded.label
                    ELSE tunnels.label
                END,
                created_at = CASE
                    WHEN tunnels.deleted_at IS NOT NULL THEN excluded.created_at
                    ELSE tunnels.created_at
                END,
                created_by = CASE
                    WHEN tunnels.deleted_at IS NOT NULL THEN excluded.created_by
                    ELSE tunnels.created_by
                END,
                deleted_at = NULL
            "#,
            params![
                id, left.wing, left.room, right.wing, right.room, label, created_at, created_by,
            ],
        )?;

        self.get_explicit_tunnel(&id)?
            .ok_or_else(|| DbError::InvalidTunnel(format!("failed to create tunnel {id}")))
    }

    pub fn list_explicit_tunnels(
        &self,
        wing: Option<&str>,
    ) -> Result<Vec<ExplicitTunnel>, DbError> {
        let wing = wing.map(str::trim).filter(|value| !value.is_empty());
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, left_wing, left_room, right_wing, right_room,
                   label, created_at, created_by, deleted_at
            FROM tunnels
            WHERE deleted_at IS NULL
              AND (?1 IS NULL OR left_wing = ?1 OR right_wing = ?1)
            ORDER BY left_wing, left_room, right_wing, right_room, id
            "#,
        )?;
        let rows = statement
            .query_map([wing], explicit_tunnel_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_explicit_tunnel(&self, tunnel_id: &str) -> Result<bool, DbError> {
        let timestamp = current_timestamp();
        let affected = self.conn.execute(
            "UPDATE tunnels SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![timestamp, tunnel_id],
        )?;
        Ok(affected > 0)
    }

    pub fn follow_explicit_tunnels(
        &self,
        from: &TunnelEndpoint,
        max_hops: u8,
    ) -> Result<Vec<TunnelFollowResult>, DbError> {
        if !(1..=2).contains(&max_hops) {
            return Err(DbError::InvalidTunnel(
                "max_hops must be 1 or 2".to_string(),
            ));
        }

        let from = normalize_tunnel_endpoint(from)?;
        let tunnels = self.list_explicit_tunnels(None)?;
        let mut visited = BTreeSet::from([from.clone()]);
        let mut queue = VecDeque::from([(from, 0_u8)]);
        let mut results = Vec::new();

        while let Some((current, hop)) = queue.pop_front() {
            if hop >= max_hops {
                continue;
            }
            let next_hop = hop + 1;
            for tunnel in &tunnels {
                let neighbor = if tunnel.left == current {
                    Some(tunnel.right.clone())
                } else if tunnel.right == current {
                    Some(tunnel.left.clone())
                } else {
                    None
                };
                let Some(neighbor) = neighbor else {
                    continue;
                };
                if !visited.insert(neighbor.clone()) {
                    continue;
                }
                results.push(TunnelFollowResult {
                    endpoint: neighbor.clone(),
                    via_tunnel_id: tunnel.id.clone(),
                    hop: next_hop,
                });
                queue.push_back((neighbor, next_hop));
            }
        }

        results.sort_by(|left, right| {
            left.hop
                .cmp(&right.hop)
                .then_with(|| left.endpoint.cmp(&right.endpoint))
                .then_with(|| left.via_tunnel_id.cmp(&right.via_tunnel_id))
        });
        Ok(results)
    }

    pub fn explicit_tunnel_hints(
        &self,
        wing: &str,
        room: Option<&str>,
    ) -> Result<Vec<String>, DbError> {
        let endpoint = TunnelEndpoint {
            wing: wing.to_string(),
            room: room.map(ToOwned::to_owned),
        };
        let hints = self
            .follow_explicit_tunnels(&endpoint, 1)?
            .into_iter()
            .map(|result| format_tunnel_endpoint(&result.endpoint))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(hints)
    }

    fn get_explicit_tunnel(&self, tunnel_id: &str) -> Result<Option<ExplicitTunnel>, DbError> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, left_wing, left_room, right_wing, right_room,
                   label, created_at, created_by, deleted_at
            FROM tunnels
            WHERE id = ?1 AND deleted_at IS NULL
            "#,
        )?;
        let mut rows = statement.query_map([tunnel_id], explicit_tunnel_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    // --- Embedding dimension management ---

    /// Returns the current embedding dimension from the vec0 table, or None if the table is empty.
    pub fn embedding_dim(&self) -> Result<Option<usize>, DbError> {
        // sqlite-vec stores dimension in table schema; probe by checking a row
        let result: std::result::Result<i64, _> = self.conn.query_row(
            "SELECT vec_length(embedding) FROM drawer_vectors LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(dim) => Ok(Some(dim as usize)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Drop and recreate the drawer_vectors table with the specified dimension.
    /// All existing vectors are lost — caller must re-embed after this.
    pub fn recreate_vectors_table(&self, dim: usize) -> Result<(), DbError> {
        self.conn.execute_batch(&format!(
            r#"
            DROP TABLE IF EXISTS drawer_vectors;
            CREATE VIRTUAL TABLE drawer_vectors USING vec0(
                id TEXT PRIMARY KEY,
                embedding FLOAT[{dim}]
            );
            "#
        ))?;
        Ok(())
    }

    /// Returns all active (non-deleted) drawer IDs and their content for re-embedding.
    pub fn all_active_drawers(&self) -> Result<Vec<(String, String)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, content FROM drawers WHERE deleted_at IS NULL ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn database_size_bytes(&self) -> Result<u64, DbError> {
        fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .map_err(|source| DbError::Metadata {
                path: self.path.clone(),
                source,
            })
    }

    pub fn schema_version(&self) -> Result<u32, DbError> {
        read_user_version(&self.conn)
    }
}

fn apply_migrations(conn: &Connection) -> Result<(), DbError> {
    let current_version = read_user_version(conn)?;
    if current_version > CURRENT_SCHEMA_VERSION {
        return Err(DbError::UnsupportedSchemaVersion {
            current: current_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    for migration in migrations()
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        apply_migration_atomic(conn, migration)?;
    }

    Ok(())
}

fn apply_migration_atomic(conn: &Connection, migration: &Migration) -> Result<(), DbError> {
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    if let Err(error) = (|| -> Result<(), DbError> {
        conn.execute_batch(migration.sql)?;
        set_user_version(conn, migration.version)?;
        conn.execute_batch("COMMIT;")?;
        Ok(())
    })() {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(error);
    }
    Ok(())
}

fn read_user_version(conn: &Connection) -> Result<u32, DbError> {
    let version = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))?;
    Ok(version)
}

fn set_user_version(conn: &Connection, version: u32) -> Result<(), DbError> {
    conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
    Ok(())
}

fn normalize_tunnel_endpoint(endpoint: &TunnelEndpoint) -> Result<TunnelEndpoint, DbError> {
    let wing = endpoint.wing.trim();
    if wing.is_empty() {
        return Err(DbError::InvalidTunnel(
            "endpoint wing is required".to_string(),
        ));
    }
    let room = endpoint
        .room
        .as_deref()
        .map(str::trim)
        .filter(|room| !room.is_empty())
        .map(ToOwned::to_owned);
    Ok(TunnelEndpoint {
        wing: wing.to_string(),
        room,
    })
}

fn explicit_tunnel_from_row(row: &Row<'_>) -> rusqlite::Result<ExplicitTunnel> {
    Ok(ExplicitTunnel {
        id: row.get(0)?,
        left: TunnelEndpoint {
            wing: row.get(1)?,
            room: row.get(2)?,
        },
        right: TunnelEndpoint {
            wing: row.get(3)?,
            room: row.get(4)?,
        },
        label: row.get(5)?,
        created_at: row.get(6)?,
        created_by: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

fn reindex_source_from_row(row: &Row<'_>) -> rusqlite::Result<ReindexSource> {
    let drawer_count = row.get::<_, i64>(3)?;
    Ok(ReindexSource {
        source_file: row.get(0)?,
        wing: row.get(1)?,
        room: row.get(2)?,
        drawer_count: drawer_count as u64,
    })
}

const V2_MIGRATION_SQL: &str = r#"
ALTER TABLE drawers ADD COLUMN deleted_at TEXT;
CREATE INDEX IF NOT EXISTS idx_drawers_deleted_at ON drawers(deleted_at);
"#;

const V3_MIGRATION_SQL: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS drawers_fts USING fts5(
    content,
    content='drawers',
    content_rowid='rowid'
);

-- Populate FTS from existing drawers (excluding soft-deleted)
INSERT INTO drawers_fts(rowid, content)
    SELECT rowid, content FROM drawers WHERE deleted_at IS NULL;

-- Keep FTS in sync: INSERT trigger
CREATE TRIGGER IF NOT EXISTS drawers_ai AFTER INSERT ON drawers BEGIN
    INSERT INTO drawers_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Keep FTS in sync: soft-delete (UPDATE deleted_at) removes from FTS
CREATE TRIGGER IF NOT EXISTS drawers_au_softdelete AFTER UPDATE OF deleted_at ON drawers
    WHEN new.deleted_at IS NOT NULL AND old.deleted_at IS NULL BEGIN
    INSERT INTO drawers_fts(drawers_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

-- No DELETE trigger on drawers — soft-deleted rows are already removed from FTS
-- by the UPDATE trigger above. Physical DELETE (purge) skips FTS because the
-- entry is already gone.
"#;

const V4_MIGRATION_SQL: &str = r#"
ALTER TABLE drawers ADD COLUMN importance INTEGER DEFAULT 0;
"#;

const V5_MIGRATION_SQL: &str = r#"
ALTER TABLE drawers ADD COLUMN memory_kind TEXT NOT NULL CHECK(memory_kind IN ('evidence', 'knowledge')) DEFAULT 'evidence';
ALTER TABLE drawers ADD COLUMN domain TEXT NOT NULL CHECK(domain IN ('project', 'agent', 'skill', 'global')) DEFAULT 'project';
ALTER TABLE drawers ADD COLUMN field TEXT NOT NULL DEFAULT 'general';
ALTER TABLE drawers ADD COLUMN anchor_kind TEXT NOT NULL CHECK(anchor_kind IN ('global', 'repo', 'worktree')) DEFAULT 'repo';
ALTER TABLE drawers ADD COLUMN anchor_id TEXT NOT NULL DEFAULT 'repo://legacy';
ALTER TABLE drawers ADD COLUMN parent_anchor_id TEXT;
ALTER TABLE drawers ADD COLUMN provenance TEXT CHECK(provenance IN ('runtime', 'research', 'human'));
ALTER TABLE drawers ADD COLUMN statement TEXT;
ALTER TABLE drawers ADD COLUMN tier TEXT CHECK(tier IN ('qi', 'shu', 'dao_ren', 'dao_tian'));
ALTER TABLE drawers ADD COLUMN status TEXT CHECK(status IN ('candidate', 'promoted', 'canonical', 'demoted', 'retired'));
ALTER TABLE drawers ADD COLUMN supporting_refs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE drawers ADD COLUMN counterexample_refs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE drawers ADD COLUMN teaching_refs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE drawers ADD COLUMN verification_refs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE drawers ADD COLUMN scope_constraints TEXT;
ALTER TABLE drawers ADD COLUMN trigger_hints TEXT;

UPDATE drawers
SET memory_kind = 'evidence',
    domain = 'project',
    field = 'general',
    anchor_kind = 'repo',
    anchor_id = 'repo://legacy',
    parent_anchor_id = NULL,
    provenance = CASE source_type
        WHEN 'project' THEN 'research'
        WHEN 'conversation' THEN 'human'
        WHEN 'manual' THEN 'human'
        ELSE NULL
    END
WHERE memory_kind = 'evidence'
  AND domain = 'project'
  AND field = 'general'
  AND anchor_kind = 'repo'
  AND anchor_id = 'repo://legacy'
  AND parent_anchor_id IS NULL
  AND provenance IS NULL;
"#;

const V6_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS tunnels (
    id TEXT PRIMARY KEY,
    left_wing TEXT NOT NULL,
    left_room TEXT,
    right_wing TEXT NOT NULL,
    right_room TEXT,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT,
    deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_tunnels_left
    ON tunnels(left_wing, left_room)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_tunnels_right
    ON tunnels(right_wing, right_room)
    WHERE deleted_at IS NULL;
"#;

const V7_MIGRATION_SQL: &str = r#"
ALTER TABLE drawers ADD COLUMN normalize_version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_drawers_normalize_version
    ON drawers(normalize_version)
    WHERE deleted_at IS NULL;
"#;

const V8_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_cards (
    id TEXT PRIMARY KEY,
    statement TEXT NOT NULL,
    content TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('qi', 'shu', 'dao_ren', 'dao_tian')),
    status TEXT NOT NULL CHECK(status IN ('candidate', 'promoted', 'canonical', 'demoted', 'retired')),
    domain TEXT NOT NULL CHECK(domain IN ('project', 'agent', 'skill', 'global')),
    field TEXT NOT NULL DEFAULT 'general',
    anchor_kind TEXT NOT NULL CHECK(anchor_kind IN ('global', 'repo', 'worktree')),
    anchor_id TEXT NOT NULL,
    parent_anchor_id TEXT,
    scope_constraints TEXT,
    trigger_hints TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_evidence_links (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL,
    evidence_drawer_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('supporting', 'verification', 'counterexample', 'teaching')),
    note TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(card_id, evidence_drawer_id, role),
    FOREIGN KEY(card_id) REFERENCES knowledge_cards(id) ON DELETE RESTRICT,
    FOREIGN KEY(evidence_drawer_id) REFERENCES drawers(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS knowledge_events (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN ('created', 'promoted', 'demoted', 'retired', 'linked', 'unlinked', 'updated', 'published_anchor')),
    from_status TEXT,
    to_status TEXT,
    reason TEXT NOT NULL,
    actor TEXT,
    metadata TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(card_id) REFERENCES knowledge_cards(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_knowledge_cards_tier_status
    ON knowledge_cards(tier, status);

CREATE INDEX IF NOT EXISTS idx_knowledge_cards_domain_field
    ON knowledge_cards(domain, field);

CREATE INDEX IF NOT EXISTS idx_knowledge_cards_anchor
    ON knowledge_cards(anchor_kind, anchor_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_evidence_links_card
    ON knowledge_evidence_links(card_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_evidence_links_evidence
    ON knowledge_evidence_links(evidence_drawer_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_events_card_created_at
    ON knowledge_events(card_id, created_at);

CREATE TRIGGER IF NOT EXISTS knowledge_events_no_update
BEFORE UPDATE ON knowledge_events
BEGIN
    SELECT RAISE(ABORT, 'knowledge_events are append-only');
END;

CREATE TRIGGER IF NOT EXISTS knowledge_events_no_delete
BEFORE DELETE ON knowledge_events
BEGIN
    SELECT RAISE(ABORT, 'knowledge_events are append-only');
END;
"#;

fn migrations() -> &'static [Migration] {
    static MIGRATIONS: &[Migration] = &[
        Migration {
            version: 1,
            sql: V1_SCHEMA_SQL,
        },
        Migration {
            version: 2,
            sql: V2_MIGRATION_SQL,
        },
        Migration {
            version: 3,
            sql: V3_MIGRATION_SQL,
        },
        Migration {
            version: 4,
            sql: V4_MIGRATION_SQL,
        },
        Migration {
            version: 5,
            sql: V5_MIGRATION_SQL,
        },
        Migration {
            version: 6,
            sql: V6_MIGRATION_SQL,
        },
        Migration {
            version: 7,
            sql: V7_MIGRATION_SQL,
        },
        Migration {
            version: 8,
            sql: V8_MIGRATION_SQL,
        },
    ];
    MIGRATIONS
}

struct Migration {
    version: u32,
    sql: &'static str,
}

fn register_sqlite_vec() -> Result<(), DbError> {
    SQLITE_VEC_AUTO_EXTENSION
        .get_or_init(|| unsafe {
            // sqlite-vec exposes a standard SQLite extension init symbol; auto-registration
            // makes vec0 available on every subsequently opened connection in this process.
            let init: rusqlite::auto_extension::RawAutoExtension =
                std::mem::transmute::<*const (), rusqlite::auto_extension::RawAutoExtension>(
                    sqlite_vec::sqlite3_vec_init as *const (),
                );

            rusqlite::auto_extension::register_auto_extension(init)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(|message| DbError::RegisterVec(message.clone()))
}

fn source_type_as_str(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Project => "project",
        SourceType::Conversation => "conversation",
        SourceType::Manual => "manual",
    }
}

fn source_type_from_str(source_type: &str) -> Result<SourceType, DbError> {
    match source_type {
        "project" => Ok(SourceType::Project),
        "conversation" => Ok(SourceType::Conversation),
        "manual" => Ok(SourceType::Manual),
        other => Err(DbError::InvalidSourceType(other.to_string())),
    }
}

fn memory_kind_as_str(memory_kind: &MemoryKind) -> &'static str {
    match memory_kind {
        MemoryKind::Evidence => "evidence",
        MemoryKind::Knowledge => "knowledge",
    }
}

fn memory_kind_from_str(memory_kind: &str) -> Result<MemoryKind, DbError> {
    match memory_kind {
        "evidence" => Ok(MemoryKind::Evidence),
        "knowledge" => Ok(MemoryKind::Knowledge),
        other => Err(DbError::InvalidEnumValue {
            kind: "memory_kind",
            value: other.to_string(),
        }),
    }
}

fn memory_domain_as_str(domain: &MemoryDomain) -> &'static str {
    match domain {
        MemoryDomain::Project => "project",
        MemoryDomain::Agent => "agent",
        MemoryDomain::Skill => "skill",
        MemoryDomain::Global => "global",
    }
}

fn memory_domain_from_str(domain: &str) -> Result<MemoryDomain, DbError> {
    match domain {
        "project" => Ok(MemoryDomain::Project),
        "agent" => Ok(MemoryDomain::Agent),
        "skill" => Ok(MemoryDomain::Skill),
        "global" => Ok(MemoryDomain::Global),
        other => Err(DbError::InvalidEnumValue {
            kind: "domain",
            value: other.to_string(),
        }),
    }
}

fn anchor_kind_as_str(anchor_kind: &AnchorKind) -> &'static str {
    match anchor_kind {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}

fn anchor_kind_from_str(anchor_kind: &str) -> Result<AnchorKind, DbError> {
    match anchor_kind {
        "global" => Ok(AnchorKind::Global),
        "repo" => Ok(AnchorKind::Repo),
        "worktree" => Ok(AnchorKind::Worktree),
        other => Err(DbError::InvalidEnumValue {
            kind: "anchor_kind",
            value: other.to_string(),
        }),
    }
}

fn provenance_as_str(provenance: &Provenance) -> &'static str {
    match provenance {
        Provenance::Runtime => "runtime",
        Provenance::Research => "research",
        Provenance::Human => "human",
    }
}

fn provenance_from_str(provenance: &str) -> Result<Provenance, DbError> {
    match provenance {
        "runtime" => Ok(Provenance::Runtime),
        "research" => Ok(Provenance::Research),
        "human" => Ok(Provenance::Human),
        other => Err(DbError::InvalidEnumValue {
            kind: "provenance",
            value: other.to_string(),
        }),
    }
}

fn knowledge_tier_as_str(tier: &KnowledgeTier) -> &'static str {
    match tier {
        KnowledgeTier::Qi => "qi",
        KnowledgeTier::Shu => "shu",
        KnowledgeTier::DaoRen => "dao_ren",
        KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn knowledge_tier_from_str(tier: &str) -> Result<KnowledgeTier, DbError> {
    match tier {
        "qi" => Ok(KnowledgeTier::Qi),
        "shu" => Ok(KnowledgeTier::Shu),
        "dao_ren" => Ok(KnowledgeTier::DaoRen),
        "dao_tian" => Ok(KnowledgeTier::DaoTian),
        other => Err(DbError::InvalidEnumValue {
            kind: "tier",
            value: other.to_string(),
        }),
    }
}

fn knowledge_status_as_str(status: &KnowledgeStatus) -> &'static str {
    match status {
        KnowledgeStatus::Candidate => "candidate",
        KnowledgeStatus::Promoted => "promoted",
        KnowledgeStatus::Canonical => "canonical",
        KnowledgeStatus::Demoted => "demoted",
        KnowledgeStatus::Retired => "retired",
    }
}

fn knowledge_status_from_str(status: &str) -> Result<KnowledgeStatus, DbError> {
    match status {
        "candidate" => Ok(KnowledgeStatus::Candidate),
        "promoted" => Ok(KnowledgeStatus::Promoted),
        "canonical" => Ok(KnowledgeStatus::Canonical),
        "demoted" => Ok(KnowledgeStatus::Demoted),
        "retired" => Ok(KnowledgeStatus::Retired),
        other => Err(DbError::InvalidEnumValue {
            kind: "status",
            value: other.to_string(),
        }),
    }
}

fn knowledge_evidence_role_as_str(role: &KnowledgeEvidenceRole) -> &'static str {
    match role {
        KnowledgeEvidenceRole::Supporting => "supporting",
        KnowledgeEvidenceRole::Verification => "verification",
        KnowledgeEvidenceRole::Counterexample => "counterexample",
        KnowledgeEvidenceRole::Teaching => "teaching",
    }
}

fn knowledge_evidence_role_from_str(role: &str) -> Result<KnowledgeEvidenceRole, DbError> {
    match role {
        "supporting" => Ok(KnowledgeEvidenceRole::Supporting),
        "verification" => Ok(KnowledgeEvidenceRole::Verification),
        "counterexample" => Ok(KnowledgeEvidenceRole::Counterexample),
        "teaching" => Ok(KnowledgeEvidenceRole::Teaching),
        other => Err(DbError::InvalidEnumValue {
            kind: "knowledge_evidence_role",
            value: other.to_string(),
        }),
    }
}

fn knowledge_event_type_as_str(event_type: &KnowledgeEventType) -> &'static str {
    match event_type {
        KnowledgeEventType::Created => "created",
        KnowledgeEventType::Promoted => "promoted",
        KnowledgeEventType::Demoted => "demoted",
        KnowledgeEventType::Retired => "retired",
        KnowledgeEventType::Linked => "linked",
        KnowledgeEventType::Unlinked => "unlinked",
        KnowledgeEventType::Updated => "updated",
        KnowledgeEventType::PublishedAnchor => "published_anchor",
    }
}

fn knowledge_event_type_from_str(event_type: &str) -> Result<KnowledgeEventType, DbError> {
    match event_type {
        "created" => Ok(KnowledgeEventType::Created),
        "promoted" => Ok(KnowledgeEventType::Promoted),
        "demoted" => Ok(KnowledgeEventType::Demoted),
        "retired" => Ok(KnowledgeEventType::Retired),
        "linked" => Ok(KnowledgeEventType::Linked),
        "unlinked" => Ok(KnowledgeEventType::Unlinked),
        "updated" => Ok(KnowledgeEventType::Updated),
        "published_anchor" => Ok(KnowledgeEventType::PublishedAnchor),
        other => Err(DbError::InvalidEnumValue {
            kind: "knowledge_event_type",
            value: other.to_string(),
        }),
    }
}

fn encode_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, DbError> {
    Ok(serde_json::to_string(value)?)
}

fn encode_optional_json<T: serde::Serialize>(value: Option<&T>) -> Result<Option<String>, DbError> {
    value.map(encode_json).transpose()
}

fn parse_string_list(raw: Option<&str>) -> Result<Vec<String>, DbError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str::<Vec<String>>(raw)?)
}

fn parse_optional_json<T>(raw: Option<&str>) -> Result<Option<T>, DbError>
where
    T: serde::de::DeserializeOwned,
{
    raw.map(serde_json::from_str)
        .transpose()
        .map_err(DbError::from)
}

fn row_decode_error(error: DbError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn knowledge_card_from_row(row: &Row<'_>) -> Result<KnowledgeCard, DbError> {
    let tier = knowledge_tier_from_str(&row.get::<_, String>(3)?)?;
    let status = knowledge_status_from_str(&row.get::<_, String>(4)?)?;
    let domain = memory_domain_from_str(&row.get::<_, String>(5)?)?;
    let anchor_kind = anchor_kind_from_str(&row.get::<_, String>(7)?)?;
    let trigger_hints = parse_optional_json(row.get::<_, Option<String>>(11)?.as_deref())?;

    anchor::validate_anchor_domain(&domain, &anchor_kind)
        .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;

    Ok(KnowledgeCard {
        id: row.get(0)?,
        statement: row.get(1)?,
        content: row.get(2)?,
        tier,
        status,
        domain,
        field: row.get(6)?,
        anchor_kind,
        anchor_id: row.get(8)?,
        parent_anchor_id: row.get(9)?,
        scope_constraints: row.get(10)?,
        trigger_hints,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn knowledge_evidence_link_from_row(row: &Row<'_>) -> Result<KnowledgeEvidenceLink, DbError> {
    Ok(KnowledgeEvidenceLink {
        id: row.get(0)?,
        card_id: row.get(1)?,
        evidence_drawer_id: row.get(2)?,
        role: knowledge_evidence_role_from_str(&row.get::<_, String>(3)?)?,
        note: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn knowledge_event_from_row(row: &Row<'_>) -> Result<KnowledgeCardEvent, DbError> {
    let from_status = row
        .get::<_, Option<String>>(3)?
        .as_deref()
        .map(knowledge_status_from_str)
        .transpose()?;
    let to_status = row
        .get::<_, Option<String>>(4)?
        .as_deref()
        .map(knowledge_status_from_str)
        .transpose()?;
    let metadata = parse_optional_json(row.get::<_, Option<String>>(7)?.as_deref())?;

    Ok(KnowledgeCardEvent {
        id: row.get(0)?,
        card_id: row.get(1)?,
        event_type: knowledge_event_type_from_str(&row.get::<_, String>(2)?)?,
        from_status,
        to_status,
        reason: row.get(5)?,
        actor: row.get(6)?,
        metadata,
        created_at: row.get(8)?,
    })
}

fn drawer_from_row(row: &Row<'_>) -> Result<Drawer, DbError> {
    let source_type = source_type_from_str(&row.get::<_, String>(5)?)?;
    let memory_kind = memory_kind_from_str(&row.get::<_, String>(10)?)?;
    let domain = memory_domain_from_str(&row.get::<_, String>(11)?)?;
    let field = row.get::<_, String>(12)?;
    let anchor_kind = anchor_kind_from_str(&row.get::<_, String>(13)?)?;
    let anchor_id = row.get::<_, String>(14)?;
    let parent_anchor_id = row.get::<_, Option<String>>(15)?;
    let provenance = row
        .get::<_, Option<String>>(16)?
        .as_deref()
        .map(provenance_from_str)
        .transpose()?;
    let statement = row.get::<_, Option<String>>(17)?;
    let tier = row
        .get::<_, Option<String>>(18)?
        .as_deref()
        .map(knowledge_tier_from_str)
        .transpose()?;
    let status = row
        .get::<_, Option<String>>(19)?
        .as_deref()
        .map(knowledge_status_from_str)
        .transpose()?;
    let supporting_refs = parse_string_list(row.get::<_, Option<String>>(20)?.as_deref())?;
    let counterexample_refs = parse_string_list(row.get::<_, Option<String>>(21)?.as_deref())?;
    let teaching_refs = parse_string_list(row.get::<_, Option<String>>(22)?.as_deref())?;
    let verification_refs = parse_string_list(row.get::<_, Option<String>>(23)?.as_deref())?;
    let scope_constraints = row.get::<_, Option<String>>(24)?;
    let trigger_hints = parse_optional_json(row.get::<_, Option<String>>(25)?.as_deref())?;

    anchor::validate_anchor_domain(&domain, &anchor_kind)
        .map_err(|message| DbError::InvalidDrawerMetadata(message.to_string()))?;

    Ok(Drawer {
        id: row.get(0)?,
        content: row.get(1)?,
        wing: row.get(2)?,
        room: row.get(3)?,
        source_file: row.get(4)?,
        source_type,
        added_at: row.get(6)?,
        chunk_index: row.get(7)?,
        normalize_version: row.get(8)?,
        importance: row.get(9)?,
        memory_kind,
        domain,
        field,
        anchor_kind,
        anchor_id,
        parent_anchor_id,
        provenance,
        statement,
        tier,
        status,
        supporting_refs,
        counterexample_refs,
        teaching_refs,
        verification_refs,
        scope_constraints,
        trigger_hints,
    })
}

fn parse_keywords(raw: Option<&str>) -> Result<Vec<String>, DbError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let value: Value = serde_json::from_str(raw)?;
    let keywords = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str())
        .map(ToOwned::to_owned)
        .collect();

    Ok(keywords)
}

fn build_fts_match_query(query: &str) -> Option<String> {
    let terms = query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_migration_rolls_back_partial_schema_changes() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            r#"
            CREATE TABLE drawers (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL
            );
            PRAGMA user_version = 4;
            "#,
        )
        .expect("create base schema");

        let migration = Migration {
            version: 5,
            sql: r#"
            ALTER TABLE drawers ADD COLUMN memory_kind TEXT;
            ALTER TABLE missing_table ADD COLUMN nope TEXT;
            "#,
        };

        let error = apply_migration_atomic(&conn, &migration).expect_err("migration should fail");
        assert!(
            matches!(error, DbError::Sqlite(_)),
            "unexpected error: {error:?}"
        );
        assert_eq!(read_user_version(&conn).expect("user_version"), 4);

        let mut stmt = conn
            .prepare("PRAGMA table_info(drawers)")
            .expect("table_info");
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query columns")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect columns");

        assert!(
            !columns.iter().any(|column| column == "memory_kind"),
            "failed migration must not leave partial columns behind"
        );
    }
}
