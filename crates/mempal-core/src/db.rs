use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::{Connection, params};
use serde_json::Value;
use thiserror::Error;

use crate::types::{Drawer, SourceType, TaxonomyEntry};

const CURRENT_SCHEMA_VERSION: u32 = 2;

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

CREATE VIRTUAL TABLE IF NOT EXISTS drawer_vectors USING vec0(
    id TEXT PRIMARY KEY,
    embedding FLOAT[384]
);

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
                chunk_index
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                drawer.id,
                drawer.content,
                drawer.wing,
                drawer.room,
                drawer.source_file,
                source_type_as_str(&drawer.source_type),
                drawer.added_at,
                drawer.chunk_index,
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

    pub fn recent_drawers(&self, limit: usize) -> Result<Vec<Drawer>, DbError> {
        let limit = i64::try_from(limit)
            .map_err(|_| rusqlite::Error::InvalidParameterName("limit".to_string()))?;
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, content, wing, room, source_file, source_type, added_at, chunk_index
            FROM drawers
            WHERE deleted_at IS NULL
            ORDER BY CAST(added_at AS INTEGER) DESC, id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut drawers = Vec::new();
        for row in rows {
            let (id, content, wing, room, source_file, source_type, added_at, chunk_index) = row?;
            drawers.push(Drawer {
                id,
                content,
                wing,
                room,
                source_file,
                source_type: source_type_from_str(&source_type)?,
                added_at,
                chunk_index,
            });
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
        let vector_json = serde_json::to_string(vector)?;
        self.conn.execute(
            "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
            (drawer_id, vector_json.as_str()),
        )?;
        Ok(())
    }

    pub fn drawer_count(&self) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM drawers WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?)
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
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, content, wing, room, source_file, source_type, added_at, chunk_index
            FROM drawers
            WHERE id = ?1 AND deleted_at IS NULL
            "#,
        )?;
        let mut rows = statement.query_map([drawer_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        match rows.next() {
            Some(row) => {
                let (id, content, wing, room, source_file, source_type, added_at, chunk_index) =
                    row?;
                Ok(Some(Drawer {
                    id,
                    content,
                    wing,
                    room,
                    source_file,
                    source_type: source_type_from_str(&source_type)?,
                    added_at,
                    chunk_index,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn soft_delete_drawer(&self, drawer_id: &str) -> Result<bool, DbError> {
        let timestamp = crate::utils::current_timestamp();
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

        for id in &ids {
            self.conn
                .execute("DELETE FROM drawer_vectors WHERE id = ?1", [id])?;
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
        conn.execute_batch(migration.sql)?;
        set_user_version(conn, migration.version)?;
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

const V2_MIGRATION_SQL: &str = r#"
ALTER TABLE drawers ADD COLUMN deleted_at TEXT;
CREATE INDEX IF NOT EXISTS idx_drawers_deleted_at ON drawers(deleted_at);
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
