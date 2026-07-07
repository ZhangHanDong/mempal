use std::env;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::core::db::CURRENT_SCHEMA_VERSION;

pub const REQUIRED_MCP_TOOLS: &[&str] = &[
    "mempal_context",
    "mempal_brief",
    "mempal_phase3",
    "mempal_session_peek",
    "mempal_cowork_bus",
];

pub const PHASE3_ACTIONS: &[&str] = &[
    "guidance",
    "instrumentation_policy",
    "prepare_record",
    "capture",
    "evaluator_advise",
    "default_proposal",
    "rollback_control",
    "check_record",
    "record_checked",
    "review",
    "readiness",
    "analytics",
    "record",
    "list",
    "stats",
    "gate",
    "research_validate_plan",
    "research_ingest_plan",
];

pub const COWORK_BUS_ACTIONS: &[&str] = &[
    "register",
    "list",
    "send",
    "broadcast",
    "drain",
    "events",
    "deliveries",
    "ack",
    "heartbeat",
    "channel_set",
    "channel_list",
    "channel_send",
    "tmux_peek",
    "doctor",
    "session_create",
    "session_list",
    "session_status",
    "session_close",
    "handoff",
    "capture",
];

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub current_version: String,
    pub supported_schema_version: u32,
    pub db: DoctorDbReport,
    pub install: DoctorInstallReport,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorDbReport {
    pub path: String,
    pub exists: bool,
    pub schema_version: Option<u32>,
    pub compatible: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorInstallReport {
    pub current_exe: Option<String>,
    pub path_mempal: Option<String>,
    pub path_matches_current_exe: Option<bool>,
}

pub fn build_doctor_report(db_path: &Path) -> DoctorReport {
    let db = inspect_db(db_path);
    let install = inspect_install();
    let mut warnings = Vec::new();
    let mut recommendations = vec![
        "Run `mempal doctor --format json` after installing or upgrading mempal.".to_string(),
        "If PATH points at an older binary, restart the MCP client after updating PATH."
            .to_string(),
    ];

    if db.exists && !db.compatible {
        warnings.push(format!(
            "database schema is not compatible with this binary: found {:?}, supported {}",
            db.schema_version, CURRENT_SCHEMA_VERSION
        ));
        recommendations
            .push("Install a mempal binary that supports this palace.db schema.".to_string());
    }
    if let Some(error) = db.error.as_deref() {
        warnings.push(format!("database schema could not be inspected: {error}"));
    }
    if install.path_matches_current_exe == Some(false) {
        warnings
            .push("PATH resolves mempal to a different executable than this process".to_string());
        recommendations
            .push("Check `which mempal` and restart long-lived MCP clients.".to_string());
    }
    if install.path_mempal.is_none() {
        warnings.push("PATH does not contain a mempal executable".to_string());
    }

    DoctorReport {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        supported_schema_version: CURRENT_SCHEMA_VERSION,
        db,
        install,
        warnings,
        recommendations,
    }
}

fn inspect_db(db_path: &Path) -> DoctorDbReport {
    if !db_path.exists() {
        return DoctorDbReport {
            path: db_path.display().to_string(),
            exists: false,
            schema_version: None,
            compatible: true,
            error: None,
        };
    }

    match read_schema_version_read_only(db_path) {
        Ok(schema_version) => DoctorDbReport {
            path: db_path.display().to_string(),
            exists: true,
            schema_version: Some(schema_version),
            compatible: schema_version <= CURRENT_SCHEMA_VERSION,
            error: None,
        },
        Err(error) => DoctorDbReport {
            path: db_path.display().to_string(),
            exists: true,
            schema_version: None,
            compatible: false,
            error: Some(error),
        },
    }
}

fn read_schema_version_read_only(db_path: &Path) -> Result<u32, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    conn.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
        .map_err(|error| error.to_string())
}

fn inspect_install() -> DoctorInstallReport {
    let current_exe = env::current_exe().ok();
    let path_mempal = find_path_executable("mempal");
    let path_matches_current_exe = match (current_exe.as_ref(), path_mempal.as_ref()) {
        (Some(current), Some(path_mempal)) => Some(paths_match(current, path_mempal)),
        (_, Some(_)) => None,
        _ => None,
    };

    DoctorInstallReport {
        current_exe: current_exe.map(|path| path.display().to_string()),
        path_mempal: path_mempal.map(|path| path.display().to_string()),
        path_matches_current_exe,
    }
}

fn find_path_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn paths_match(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
