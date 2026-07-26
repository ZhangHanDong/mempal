//! Multi-agent cowork bus for concrete agent instances.
//!
//! P8 routes by tool family (`claude` / `codex`). P84 adds `agent_id`
//! addressing so multiple instances of the same tool can work in one project
//! without racing on a shared inbox.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::core::{
    db::{Database, DbError},
    types::{BootstrapEvidenceArgs, Drawer, SourceType},
    utils::{build_bootstrap_evidence_drawer_id, current_timestamp},
};

use super::inbox::{
    InboxMessage, MAX_MESSAGE_SIZE, MAX_PENDING_MESSAGES, MAX_TOTAL_INBOX_BYTES,
    encode_project_identity, project_identity,
};
use super::peek::{format_rfc3339, parse_rfc3339};

const MAX_EVENT_MESSAGE_PREVIEW_CHARS: usize = 200;
const PRESENCE_STALE_AFTER_SECONDS: i64 = 10 * 60;

static BUS_EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct MessageMetadata<'a> {
    thread_id: Option<&'a str>,
    channel: Option<&'a str>,
}

#[derive(Debug, Error)]
pub enum BusError {
    #[error("invalid agent id `{0}`: expected 1-64 ASCII letters, digits, `_`, `-`, or `.`")]
    InvalidAgentId(String),
    #[error("invalid tool name `{0}`: expected 1-64 ASCII letters, digits, `_`, `-`, or `.`")]
    InvalidTool(String),
    #[error("unsupported transport `{0}`: expected inbox|tmux")]
    UnsupportedTransport(String),
    #[error("invalid channel `{0}`: expected 1-64 ASCII letters, digits, `_`, `-`, or `.`")]
    InvalidChannel(String),
    #[error("invalid thread id `{0}`: expected 1-64 ASCII letters, digits, `_`, `-`, or `.`")]
    InvalidThreadId(String),
    #[error("unknown channel `{0}`")]
    UnknownChannel(String),
    #[error("channel `{0}` must contain at least one agent")]
    EmptyChannel(String),
    #[error("tmux_target is required when transport=tmux")]
    TmuxTargetRequired,
    #[error("tmux delivery failed with status {0}")]
    TmuxFailed(String),
    #[error("tmux capture failed with status {0}")]
    TmuxCaptureFailed(String),
    #[error("tmux probe failed with status {0}")]
    TmuxProbeFailed(String),
    #[error("agent `{0}` is not registered with transport=tmux")]
    NotTmuxAgent(String),
    #[error("invalid tmux capture line count `{0}`: expected 1..=500")]
    InvalidLineCount(usize),
    #[error("invalid session id `{0}`: expected 1-64 ASCII letters, digits, `_`, `-`, or `.`")]
    InvalidSessionId(String),
    #[error("session `{0}` must contain at least one agent")]
    EmptySession(String),
    #[error("unknown session `{0}`")]
    UnknownSession(String),
    #[error("unsupported session status `{0}`: expected active|paused|closed")]
    InvalidSessionStatus(String),
    #[error("unsupported cowork capture summary source `{0}`: expected handoff")]
    UnsupportedCaptureSource(String),
    #[error("capture execute requires an open database")]
    MissingCaptureDatabase,
    #[error("invalid timestamp `{0}`: expected RFC3339")]
    InvalidTimestamp(String),
    #[error("unknown delivery message id `{0}`")]
    UnknownDelivery(String),
    #[error("delivery `{message_id}` is addressed to `{target_agent_id}`, not `{agent_id}`")]
    DeliveryTargetMismatch {
        message_id: String,
        target_agent_id: String,
        agent_id: String,
    },
    #[error("cannot ack failed delivery `{0}`")]
    CannotAckFailed(String),
    #[error("unknown source agent `{0}`")]
    UnknownSource(String),
    #[error("unknown target agent `{0}`")]
    UnknownTarget(String),
    #[error("unknown agent `{0}`")]
    UnknownAgent(String),
    #[error("cannot send to self `{0}`")]
    SelfSend(String),
    #[error("message content exceeds {MAX_MESSAGE_SIZE} bytes: got {0} bytes")]
    MessageTooLarge(usize),
    #[error(
        "agent inbox full: {current_count} messages / {current_bytes} bytes pending \
         (limits: {MAX_PENDING_MESSAGES} messages, {MAX_TOTAL_INBOX_BYTES} bytes)"
    )]
    InboxFull {
        current_count: usize,
        current_bytes: u64,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("db error: {0}")]
    Db(#[from] DbError),
    #[error("legacy inbox error: {0}")]
    LegacyInbox(#[from] super::inbox::InboxError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub agent_id: String,
    pub tool: String,
    pub transport: String,
    pub tmux_target: Option<String>,
    pub registered_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRegistry {
    pub agents: BTreeMap<String, AgentRecord>,
    #[serde(default)]
    pub channels: BTreeMap<String, ChannelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRecord {
    pub channel: String,
    pub agents: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    pub tool: String,
    pub transport: String,
    pub tmux_target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub from: String,
    pub targets: Vec<String>,
    pub message: String,
    pub operation: SendOperation,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum SendOperation {
    Send,
    Broadcast,
}

impl SendOperation {
    fn event_type(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Broadcast => "broadcast",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendReport {
    pub delivered: Vec<DeliveryReport>,
}

#[derive(Debug, Clone)]
pub struct DeliveryReport {
    pub message_id: String,
    pub target_agent_id: String,
    pub transport: String,
    pub inbox_path: Option<PathBuf>,
    pub inbox_size_after: Option<u64>,
    pub tmux_target: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub record: AgentRecord,
    pub presence: String,
    pub pending_count: usize,
    pub pending_bytes: u64,
    pub preview: Vec<InboxMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEvent {
    pub event_id: String,
    pub occurred_at: String,
    pub event_type: String,
    pub status: String,
    pub actor_agent_id: Option<String>,
    pub target_agent_ids: Vec<String>,
    pub transport: Option<String>,
    pub message_preview: Option<String>,
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStatus {
    pub message_id: String,
    pub event_type: String,
    pub status: String,
    pub from: String,
    pub target_agent_id: String,
    pub transport: String,
    pub message_preview: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub delivered_at: String,
    pub updated_at: String,
    pub acked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxPeek {
    pub agent_id: String,
    pub tmux_target: String,
    pub lines: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub status: String,
    pub agent_count: usize,
    pub channel_count: usize,
    pub session_count: usize,
    pub stale_agents: usize,
    pub never_seen_agents: usize,
    pub pending_deliveries: usize,
    pub warnings: Vec<String>,
    pub tmux: Vec<TmuxProbeReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxProbeReport {
    pub agent_id: String,
    pub tmux_target: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionRegistry {
    pub sessions: BTreeMap<String, TeamSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSession {
    pub session_id: String,
    pub title: String,
    pub goal: Option<String>,
    pub agents: Vec<String>,
    pub channels: Vec<String>,
    pub thread_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub session_id: String,
    pub title: String,
    pub goal: Option<String>,
    pub agents: Vec<String>,
    pub channels: Vec<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HandoffFilters {
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSummary {
    pub filters: HandoffSummaryFilters,
    pub sessions: Vec<TeamSession>,
    pub agents: Vec<AgentStatusSummary>,
    pub pending_deliveries: Vec<DeliveryStatus>,
    pub recent_events: Vec<BusEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSummaryFilters {
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusSummary {
    pub agent_id: String,
    pub tool: String,
    pub presence: String,
    pub pending_count: usize,
}

#[derive(Debug, Clone)]
pub struct CoworkCaptureRequest {
    pub summary_source: String,
    pub wing: String,
    pub room: Option<String>,
    pub thread_id: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub note: Option<String>,
    pub execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoworkCaptureReport {
    pub writes: bool,
    pub drawer_id: Option<String>,
    pub wing: String,
    pub room: Option<String>,
    pub source: String,
    pub content: String,
}

pub fn validate_agent_id(agent_id: &str) -> Result<(), BusError> {
    if is_safe_bus_token(agent_id) {
        Ok(())
    } else {
        Err(BusError::InvalidAgentId(agent_id.to_string()))
    }
}

fn validate_tool(tool: &str) -> Result<(), BusError> {
    if is_safe_bus_token(tool) {
        Ok(())
    } else {
        Err(BusError::InvalidTool(tool.to_string()))
    }
}

fn validate_channel(channel: &str) -> Result<(), BusError> {
    if is_safe_bus_token(channel) {
        Ok(())
    } else {
        Err(BusError::InvalidChannel(channel.to_string()))
    }
}

fn validate_thread_id(thread_id: &str) -> Result<(), BusError> {
    if is_safe_bus_token(thread_id) {
        Ok(())
    } else {
        Err(BusError::InvalidThreadId(thread_id.to_string()))
    }
}

fn validate_session_id(session_id: &str) -> Result<(), BusError> {
    if is_safe_bus_token(session_id) {
        Ok(())
    } else {
        Err(BusError::InvalidSessionId(session_id.to_string()))
    }
}

fn validate_session_status(status: &str) -> Result<(), BusError> {
    match status {
        "active" | "paused" | "closed" => Ok(()),
        other => Err(BusError::InvalidSessionStatus(other.to_string())),
    }
}

fn is_safe_bus_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

pub fn project_bus_dir(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, BusError> {
    let identity = project_identity(cwd);
    let encoded = encode_project_identity(&identity)?;
    Ok(mempal_home.join("cowork-bus").join(encoded))
}

pub fn registry_path(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, BusError> {
    Ok(project_bus_dir(mempal_home, cwd)?.join("agents.json"))
}

pub fn agent_inbox_path(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
) -> Result<PathBuf, BusError> {
    validate_agent_id(agent_id)?;
    Ok(project_bus_dir(mempal_home, cwd)?
        .join("inbox")
        .join(format!("{agent_id}.jsonl")))
}

pub fn events_path(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, BusError> {
    Ok(project_bus_dir(mempal_home, cwd)?.join("events.jsonl"))
}

pub fn sessions_path(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, BusError> {
    Ok(project_bus_dir(mempal_home, cwd)?.join("sessions.json"))
}

pub fn load_registry(mempal_home: &Path, cwd: &Path) -> Result<AgentRegistry, BusError> {
    let path = registry_path(mempal_home, cwd)?;
    if !path.exists() {
        return Ok(AgentRegistry::default());
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(AgentRegistry::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn save_registry(mempal_home: &Path, cwd: &Path, registry: &AgentRegistry) -> Result<(), BusError> {
    let path = registry_path(mempal_home, cwd)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(registry)?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn load_sessions(mempal_home: &Path, cwd: &Path) -> Result<SessionRegistry, BusError> {
    let path = sessions_path(mempal_home, cwd)?;
    if !path.exists() {
        return Ok(SessionRegistry::default());
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(SessionRegistry::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn save_sessions(
    mempal_home: &Path,
    cwd: &Path,
    sessions: &SessionRegistry,
) -> Result<(), BusError> {
    let path = sessions_path(mempal_home, cwd)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(sessions)?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn register_agent(
    mempal_home: &Path,
    cwd: &Path,
    request: RegisterAgentRequest,
) -> Result<AgentRecord, BusError> {
    validate_agent_id(&request.agent_id)?;
    validate_tool(&request.tool)?;
    if request.transport != "inbox" && request.transport != "tmux" {
        return Err(BusError::UnsupportedTransport(request.transport));
    }
    if request.transport == "tmux"
        && request
            .tmux_target
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(BusError::TmuxTargetRequired);
    }

    let now = format_rfc3339(SystemTime::now());
    let mut registry = load_registry(mempal_home, cwd)?;
    let registered_at = registry
        .agents
        .get(&request.agent_id)
        .map(|existing| existing.registered_at.clone())
        .unwrap_or_else(|| now.clone());
    let record = AgentRecord {
        agent_id: request.agent_id.clone(),
        tool: request.tool,
        transport: request.transport,
        tmux_target: request.tmux_target,
        registered_at,
        updated_at: now.clone(),
        last_seen_at: Some(now),
    };
    registry
        .agents
        .insert(request.agent_id.clone(), record.clone());
    save_registry(mempal_home, cwd, &registry)?;
    let mut details = BTreeMap::new();
    details.insert("tool".to_string(), record.tool.clone());
    if let Some(tmux_target) = &record.tmux_target {
        details.insert("tmux_target".to_string(), tmux_target.clone());
    }
    append_bus_event(
        mempal_home,
        cwd,
        "register",
        "registered",
        Some(record.agent_id.clone()),
        vec![record.agent_id.clone()],
        Some(record.transport.clone()),
        None,
        details,
    )?;
    Ok(record)
}

pub fn list_agent_status(mempal_home: &Path, cwd: &Path) -> Result<Vec<AgentStatus>, BusError> {
    list_agent_status_at(mempal_home, cwd, None)
}

pub fn list_agent_status_at(
    mempal_home: &Path,
    cwd: &Path,
    now: Option<&str>,
) -> Result<Vec<AgentStatus>, BusError> {
    let now_seconds = match now {
        Some(now) => {
            parse_rfc3339(now).ok_or_else(|| BusError::InvalidTimestamp(now.to_string()))?
        }
        None => current_unix_seconds(),
    };
    let registry = load_registry(mempal_home, cwd)?;
    let mut statuses = Vec::new();
    for record in registry.agents.values() {
        let path = agent_inbox_path(mempal_home, cwd, &record.agent_id)?;
        let (pending_count, pending_bytes, preview) = read_inbox_stats(&path)?;
        let presence = presence_for(record.last_seen_at.as_deref(), now_seconds);
        statuses.push(AgentStatus {
            record: record.clone(),
            presence,
            pending_count,
            pending_bytes,
            preview,
        });
    }
    Ok(statuses)
}

pub fn heartbeat_agent(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
    seen_at: Option<&str>,
) -> Result<AgentRecord, BusError> {
    validate_agent_id(agent_id)?;
    let seen_at = match seen_at {
        Some(seen_at) => {
            parse_rfc3339(seen_at)
                .ok_or_else(|| BusError::InvalidTimestamp(seen_at.to_string()))?;
            seen_at.to_string()
        }
        None => format_rfc3339(SystemTime::now()),
    };
    let mut registry = load_registry(mempal_home, cwd)?;
    let record = registry
        .agents
        .get_mut(agent_id)
        .ok_or_else(|| BusError::UnknownAgent(agent_id.to_string()))?;
    record.last_seen_at = Some(seen_at.clone());
    record.updated_at = seen_at.clone();
    let record = record.clone();
    save_registry(mempal_home, cwd, &registry)?;

    let mut details = BTreeMap::new();
    details.insert("last_seen_at".to_string(), seen_at);
    append_bus_event(
        mempal_home,
        cwd,
        "heartbeat",
        "seen",
        Some(agent_id.to_string()),
        vec![agent_id.to_string()],
        Some(record.transport.clone()),
        None,
        details,
    )?;
    Ok(record)
}

pub fn set_channel(
    mempal_home: &Path,
    cwd: &Path,
    channel: &str,
    agents: Vec<String>,
) -> Result<ChannelRecord, BusError> {
    validate_channel(channel)?;
    let agents = dedup_targets(agents)?;
    if agents.is_empty() {
        return Err(BusError::EmptyChannel(channel.to_string()));
    }
    let mut registry = load_registry(mempal_home, cwd)?;
    for agent_id in &agents {
        if !registry.agents.contains_key(agent_id) {
            return Err(BusError::UnknownAgent(agent_id.clone()));
        }
    }
    let record = ChannelRecord {
        channel: channel.to_string(),
        agents,
        updated_at: format_rfc3339(SystemTime::now()),
    };
    registry
        .channels
        .insert(channel.to_string(), record.clone());
    save_registry(mempal_home, cwd, &registry)?;
    let mut details = BTreeMap::new();
    details.insert("channel".to_string(), channel.to_string());
    details.insert("agents".to_string(), record.agents.join(","));
    append_bus_event(
        mempal_home,
        cwd,
        "channel_set",
        "updated",
        None,
        record.agents.clone(),
        None,
        None,
        details,
    )?;
    Ok(record)
}

pub fn list_channels(mempal_home: &Path, cwd: &Path) -> Result<Vec<ChannelRecord>, BusError> {
    let registry = load_registry(mempal_home, cwd)?;
    Ok(registry.channels.values().cloned().collect())
}

pub fn create_session(
    mempal_home: &Path,
    cwd: &Path,
    request: CreateSessionRequest,
) -> Result<TeamSession, BusError> {
    validate_session_id(&request.session_id)?;
    if let Some(thread_id) = &request.thread_id {
        validate_thread_id(thread_id)?;
    }
    let agents = dedup_targets(request.agents)?;
    if agents.is_empty() {
        return Err(BusError::EmptySession(request.session_id));
    }

    let registry = load_registry(mempal_home, cwd)?;
    for agent_id in &agents {
        if !registry.agents.contains_key(agent_id) {
            return Err(BusError::UnknownAgent(agent_id.clone()));
        }
    }
    let mut channels = Vec::new();
    let mut seen_channels = BTreeSet::new();
    for channel in request.channels {
        validate_channel(&channel)?;
        if !registry.channels.contains_key(&channel) {
            return Err(BusError::UnknownChannel(channel));
        }
        if seen_channels.insert(channel.clone()) {
            channels.push(channel);
        }
    }

    let now = format_rfc3339(SystemTime::now());
    let mut sessions = load_sessions(mempal_home, cwd)?;
    let created_at = sessions
        .sessions
        .get(&request.session_id)
        .map(|existing| existing.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let session = TeamSession {
        session_id: request.session_id.clone(),
        title: request.title,
        goal: request.goal,
        agents,
        channels,
        thread_id: request.thread_id,
        status: "active".to_string(),
        created_at,
        updated_at: now,
    };
    sessions
        .sessions
        .insert(request.session_id.clone(), session.clone());
    save_sessions(mempal_home, cwd, &sessions)?;

    let mut details = BTreeMap::new();
    details.insert("session_id".to_string(), session.session_id.clone());
    details.insert("title".to_string(), session.title.clone());
    details.insert("status".to_string(), session.status.clone());
    if let Some(thread_id) = &session.thread_id {
        details.insert("thread_id".to_string(), thread_id.clone());
    }
    append_bus_event(
        mempal_home,
        cwd,
        "session_create",
        "created",
        None,
        session.agents.clone(),
        None,
        None,
        details,
    )?;
    Ok(session)
}

pub fn list_sessions(mempal_home: &Path, cwd: &Path) -> Result<Vec<TeamSession>, BusError> {
    let sessions = load_sessions(mempal_home, cwd)?;
    Ok(sessions.sessions.values().cloned().collect())
}

pub fn update_session_status(
    mempal_home: &Path,
    cwd: &Path,
    session_id: &str,
    status: &str,
) -> Result<TeamSession, BusError> {
    validate_session_id(session_id)?;
    validate_session_status(status)?;
    let mut sessions = load_sessions(mempal_home, cwd)?;
    let session = sessions
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| BusError::UnknownSession(session_id.to_string()))?;
    session.status = status.to_string();
    session.updated_at = format_rfc3339(SystemTime::now());
    let session = session.clone();
    save_sessions(mempal_home, cwd, &sessions)?;

    let mut details = BTreeMap::new();
    details.insert("session_id".to_string(), session.session_id.clone());
    details.insert("status".to_string(), session.status.clone());
    append_bus_event(
        mempal_home,
        cwd,
        "session_status",
        status,
        None,
        session.agents.clone(),
        None,
        None,
        details,
    )?;
    Ok(session)
}

pub fn send_channel(
    mempal_home: &Path,
    cwd: &Path,
    from: String,
    channel: String,
    message: String,
    thread_id: Option<String>,
) -> Result<SendReport, BusError> {
    validate_channel(&channel)?;
    let registry = load_registry(mempal_home, cwd)?;
    let record = registry
        .channels
        .get(&channel)
        .ok_or_else(|| BusError::UnknownChannel(channel.clone()))?;
    send(
        mempal_home,
        cwd,
        SendRequest {
            from,
            targets: record.agents.clone(),
            message,
            operation: SendOperation::Broadcast,
            thread_id,
            channel: Some(channel),
        },
    )
}

pub fn tmux_peek_agent(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
    lines: usize,
) -> Result<TmuxPeek, BusError> {
    validate_agent_id(agent_id)?;
    if !(1..=500).contains(&lines) {
        return Err(BusError::InvalidLineCount(lines));
    }
    let registry = load_registry(mempal_home, cwd)?;
    let record = registry
        .agents
        .get(agent_id)
        .ok_or_else(|| BusError::UnknownAgent(agent_id.to_string()))?;
    if record.transport != "tmux" {
        return Err(BusError::NotTmuxAgent(agent_id.to_string()));
    }
    let tmux_target = record
        .tmux_target
        .as_deref()
        .ok_or(BusError::TmuxTargetRequired)?;
    let content = capture_tmux(tmux_target, lines)?;
    Ok(TmuxPeek {
        agent_id: agent_id.to_string(),
        tmux_target: tmux_target.to_string(),
        lines,
        content,
    })
}

pub fn doctor(
    mempal_home: &Path,
    cwd: &Path,
    now: Option<&str>,
    probe_tmux: bool,
) -> Result<DoctorReport, BusError> {
    let statuses = list_agent_status_at(mempal_home, cwd, now)?;
    let channels = list_channels(mempal_home, cwd)?;
    let sessions = list_sessions(mempal_home, cwd)?;
    let deliveries = list_delivery_statuses(mempal_home, cwd, None)?;
    let pending_deliveries = deliveries
        .iter()
        .filter(|delivery| delivery.status == "pending")
        .count();
    let stale_agents = statuses
        .iter()
        .filter(|status| status.presence == "stale")
        .count();
    let never_seen_agents = statuses
        .iter()
        .filter(|status| status.presence == "never_seen")
        .count();

    let mut warnings = Vec::new();
    if statuses.is_empty() {
        warnings.push("no registered agents".to_string());
    }
    if stale_agents > 0 {
        warnings.push(format!("stale agents: {stale_agents}"));
    }
    if pending_deliveries > 0 {
        warnings.push(format!("pending deliveries: {pending_deliveries}"));
    }
    if never_seen_agents > 0 {
        warnings.push(format!("never seen agents: {never_seen_agents}"));
    }

    let mut tmux = Vec::new();
    for status in &statuses {
        if status.record.transport != "tmux" {
            continue;
        }
        let Some(tmux_target) = status.record.tmux_target.clone() else {
            warnings.push(format!(
                "tmux agent {} has no target",
                status.record.agent_id
            ));
            tmux.push(TmuxProbeReport {
                agent_id: status.record.agent_id.clone(),
                tmux_target: String::new(),
                status: "missing_target".to_string(),
                detail: None,
            });
            continue;
        };
        let probe = if probe_tmux {
            probe_tmux_target(&tmux_target)
        } else {
            TmuxProbeReport {
                agent_id: status.record.agent_id.clone(),
                tmux_target: tmux_target.clone(),
                status: "not_probed".to_string(),
                detail: None,
            }
        };
        if probe.status == "failed" {
            warnings.push(format!(
                "tmux target failed: {} {}",
                status.record.agent_id, tmux_target
            ));
        }
        tmux.push(TmuxProbeReport {
            agent_id: status.record.agent_id.clone(),
            tmux_target,
            ..probe
        });
    }

    let status = if warnings.is_empty() {
        "ok".to_string()
    } else {
        "warning".to_string()
    };
    Ok(DoctorReport {
        status,
        agent_count: statuses.len(),
        channel_count: channels.len(),
        session_count: sessions.len(),
        stale_agents,
        never_seen_agents,
        pending_deliveries,
        warnings,
        tmux,
    })
}

pub fn build_handoff_summary(
    mempal_home: &Path,
    cwd: &Path,
    filters: HandoffFilters,
) -> Result<HandoffSummary, BusError> {
    if let Some(thread_id) = &filters.thread_id {
        validate_thread_id(thread_id)?;
    }
    if let Some(channel) = &filters.channel {
        validate_channel(channel)?;
    }
    if let Some(session_id) = &filters.session_id {
        validate_session_id(session_id)?;
    }
    let limit = filters.limit.unwrap_or(20);
    let sessions = list_sessions(mempal_home, cwd)?
        .into_iter()
        .filter(|session| {
            filters
                .session_id
                .as_ref()
                .is_none_or(|session_id| &session.session_id == session_id)
        })
        .filter(|session| {
            filters
                .thread_id
                .as_ref()
                .is_none_or(|thread_id| session.thread_id.as_ref() == Some(thread_id))
        })
        .filter(|session| {
            filters
                .channel
                .as_ref()
                .is_none_or(|channel| session.channels.iter().any(|item| item == channel))
        })
        .collect::<Vec<_>>();
    let agents = list_agent_status(mempal_home, cwd)?
        .into_iter()
        .map(|status| AgentStatusSummary {
            agent_id: status.record.agent_id,
            tool: status.record.tool,
            presence: status.presence,
            pending_count: status.pending_count,
        })
        .collect::<Vec<_>>();
    let pending_deliveries = list_delivery_statuses(mempal_home, cwd, None)?
        .into_iter()
        .filter(|delivery| delivery.status == "pending")
        .filter(|delivery| {
            filters
                .thread_id
                .as_ref()
                .is_none_or(|thread_id| delivery.thread_id.as_ref() == Some(thread_id))
        })
        .filter(|delivery| {
            filters
                .channel
                .as_ref()
                .is_none_or(|channel| delivery.channel.as_ref() == Some(channel))
        })
        .collect::<Vec<_>>();
    let mut recent_events = list_events(mempal_home, cwd, Some(limit))?
        .into_iter()
        .filter(|event| {
            filters.thread_id.as_ref().is_none_or(|thread_id| {
                event.details.get("thread_id") == Some(thread_id)
                    || event.details.get("session_id").is_some_and(|session_id| {
                        sessions.iter().any(|session| {
                            &session.session_id == session_id
                                && session.thread_id.as_ref() == Some(thread_id)
                        })
                    })
            })
        })
        .filter(|event| {
            filters
                .channel
                .as_ref()
                .is_none_or(|channel| event.details.get("channel") == Some(channel))
        })
        .filter(|event| {
            filters
                .session_id
                .as_ref()
                .is_none_or(|session_id| event.details.get("session_id") == Some(session_id))
        })
        .collect::<Vec<_>>();
    if recent_events.len() > limit {
        recent_events.drain(0..recent_events.len() - limit);
    }

    Ok(HandoffSummary {
        filters: HandoffSummaryFilters {
            thread_id: filters.thread_id,
            channel: filters.channel,
            session_id: filters.session_id,
            limit,
        },
        sessions,
        agents,
        pending_deliveries,
        recent_events,
    })
}

pub fn capture_handoff_to_memory(
    db: Option<&Database>,
    mempal_home: &Path,
    cwd: &Path,
    request: CoworkCaptureRequest,
) -> Result<CoworkCaptureReport, BusError> {
    if request.summary_source != "handoff" {
        return Err(BusError::UnsupportedCaptureSource(request.summary_source));
    }
    let summary = build_handoff_summary(
        mempal_home,
        cwd,
        HandoffFilters {
            thread_id: request.thread_id,
            channel: request.channel,
            session_id: request.session_id,
            limit: Some(50),
        },
    )?;
    let capture_id = capture_id();
    let mut content = String::new();
    content.push_str("# Cowork Handoff Capture\n\n");
    content.push_str(&format!("capture_id: {capture_id}\n"));
    content.push_str(&format!("project: {}\n", cwd.display()));
    content.push_str("summary_source: handoff\n\n");
    if let Some(note) = request.note.as_deref() {
        content.push_str("## Note\n\n");
        content.push_str(note);
        content.push_str("\n\n");
    }
    content.push_str("## Handoff Summary\n\n");
    content.push_str(&format_handoff_plain(&summary));

    let source_type = SourceType::Manual;
    let source_file = format!("cowork-capture://{capture_id}");
    let drawer_id = build_bootstrap_evidence_drawer_id(
        &request.wing,
        request.room.as_deref(),
        &content,
        &source_type,
        Some(source_file.as_str()),
    );
    if request.execute {
        let db = db.ok_or(BusError::MissingCaptureDatabase)?;
        let drawer = Drawer::new_bootstrap_evidence(BootstrapEvidenceArgs {
            id: drawer_id.clone(),
            content: content.clone(),
            wing: request.wing.clone(),
            room: request.room.clone(),
            source_file: Some(source_file),
            source_type,
            added_at: current_timestamp(),
            chunk_index: Some(0),
            importance: 3,
        });
        db.insert_drawer(&drawer)?;
    }

    Ok(CoworkCaptureReport {
        writes: request.execute,
        drawer_id: Some(drawer_id),
        wing: request.wing,
        room: request.room,
        source: "handoff".to_string(),
        content,
    })
}

pub fn send(mempal_home: &Path, cwd: &Path, request: SendRequest) -> Result<SendReport, BusError> {
    validate_agent_id(&request.from)?;
    if let Some(thread_id) = &request.thread_id {
        validate_thread_id(thread_id)?;
    }
    if let Some(channel) = &request.channel {
        validate_channel(channel)?;
    }
    if request.message.len() > MAX_MESSAGE_SIZE {
        return Err(BusError::MessageTooLarge(request.message.len()));
    }

    let targets = dedup_targets(request.targets)?;
    if targets.iter().any(|target| target == &request.from) {
        return Err(BusError::SelfSend(request.from));
    }

    let registry = load_registry(mempal_home, cwd)?;
    let source = registry
        .agents
        .get(&request.from)
        .ok_or_else(|| BusError::UnknownSource(request.from.clone()))?;
    let mut target_records = Vec::new();
    for target in &targets {
        validate_agent_id(target)?;
        let record = registry
            .agents
            .get(target)
            .ok_or_else(|| BusError::UnknownTarget(target.clone()))?;
        target_records.push(record.clone());
    }

    let pushed_at = format_rfc3339(SystemTime::now());
    let event_type = request.operation.event_type();
    let message_preview = Some(message_preview(&request.message));
    let mut delivered = Vec::new();
    for target in target_records {
        match target.transport.as_str() {
            "inbox" => {
                let (inbox_path, inbox_size_after) = append_to_agent_inbox(
                    mempal_home,
                    cwd,
                    &source.agent_id,
                    &target.agent_id,
                    &request.message,
                    &pushed_at,
                    MessageMetadata {
                        thread_id: request.thread_id.as_deref(),
                        channel: request.channel.as_deref(),
                    },
                )?;
                let mut details = BTreeMap::new();
                details.insert(
                    "inbox_path".to_string(),
                    inbox_path.to_string_lossy().to_string(),
                );
                details.insert("inbox_size_after".to_string(), inbox_size_after.to_string());
                add_optional_detail(&mut details, "thread_id", request.thread_id.as_deref());
                add_optional_detail(&mut details, "channel", request.channel.as_deref());
                let event = append_bus_event(
                    mempal_home,
                    cwd,
                    event_type,
                    "delivered",
                    Some(source.agent_id.clone()),
                    vec![target.agent_id.clone()],
                    Some("inbox".to_string()),
                    message_preview.clone(),
                    details,
                )?;
                delivered.push(DeliveryReport {
                    message_id: event.event_id,
                    target_agent_id: target.agent_id,
                    transport: "inbox".to_string(),
                    inbox_path: Some(inbox_path),
                    inbox_size_after: Some(inbox_size_after),
                    tmux_target: None,
                    thread_id: request.thread_id.clone(),
                    channel: request.channel.clone(),
                });
            }
            "tmux" => {
                let tmux_target = target
                    .tmux_target
                    .as_deref()
                    .ok_or(BusError::TmuxTargetRequired)?;
                if let Err(err) = send_tmux(
                    &source.agent_id,
                    &target.agent_id,
                    tmux_target,
                    &request.message,
                ) {
                    let mut details = BTreeMap::new();
                    details.insert("tmux_target".to_string(), tmux_target.to_string());
                    details.insert("error".to_string(), err.to_string());
                    add_optional_detail(&mut details, "thread_id", request.thread_id.as_deref());
                    add_optional_detail(&mut details, "channel", request.channel.as_deref());
                    let _ = append_bus_event(
                        mempal_home,
                        cwd,
                        event_type,
                        "failed",
                        Some(source.agent_id.clone()),
                        vec![target.agent_id.clone()],
                        Some("tmux".to_string()),
                        message_preview.clone(),
                        details,
                    );
                    return Err(err);
                }
                let mut details = BTreeMap::new();
                details.insert("tmux_target".to_string(), tmux_target.to_string());
                add_optional_detail(&mut details, "thread_id", request.thread_id.as_deref());
                add_optional_detail(&mut details, "channel", request.channel.as_deref());
                let event = append_bus_event(
                    mempal_home,
                    cwd,
                    event_type,
                    "delivered",
                    Some(source.agent_id.clone()),
                    vec![target.agent_id.clone()],
                    Some("tmux".to_string()),
                    message_preview.clone(),
                    details,
                )?;
                delivered.push(DeliveryReport {
                    message_id: event.event_id,
                    target_agent_id: target.agent_id,
                    transport: "tmux".to_string(),
                    inbox_path: None,
                    inbox_size_after: None,
                    tmux_target: Some(tmux_target.to_string()),
                    thread_id: request.thread_id.clone(),
                    channel: request.channel.clone(),
                });
            }
            other => return Err(BusError::UnsupportedTransport(other.to_string())),
        }
    }
    Ok(SendReport { delivered })
}

fn dedup_targets(targets: Vec<String>) -> Result<Vec<String>, BusError> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for target in targets {
        validate_agent_id(&target)?;
        if seen.insert(target.clone()) {
            deduped.push(target);
        }
    }
    Ok(deduped)
}

fn append_to_agent_inbox(
    mempal_home: &Path,
    cwd: &Path,
    from: &str,
    target: &str,
    content: &str,
    pushed_at: &str,
    metadata: MessageMetadata<'_>,
) -> Result<(PathBuf, u64), BusError> {
    use std::io::Write;

    let path = agent_inbox_path(mempal_home, cwd, target)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let (existing_count, existing_bytes, _) = read_inbox_stats(&path)?;
    let msg = InboxMessage {
        pushed_at: pushed_at.to_string(),
        from: from.to_string(),
        content: content.to_string(),
        thread_id: metadata.thread_id.map(str::to_string),
        channel: metadata.channel.map(str::to_string),
        message_id: None,
    };
    let line = serde_json::to_string(&msg)?;
    let prospective_count = existing_count + 1;
    let prospective_bytes = existing_bytes.saturating_add(line.len() as u64 + 1);
    if prospective_count > MAX_PENDING_MESSAGES || prospective_bytes > MAX_TOTAL_INBOX_BYTES {
        return Err(BusError::InboxFull {
            current_count: existing_count,
            current_bytes: existing_bytes,
        });
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    file.flush()?;
    let size = std::fs::metadata(&path)?.len();
    Ok((path, size))
}

fn send_tmux(from: &str, target: &str, tmux_target: &str, content: &str) -> Result<(), BusError> {
    let envelope = format!("[mempal bus from {from} to {target}] {content}");
    let status = std::process::Command::new("tmux")
        .args(["send-keys", "-t", tmux_target, "--", &envelope, "Enter"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(BusError::TmuxFailed(
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
        ))
    }
}

fn capture_tmux(tmux_target: &str, lines: usize) -> Result<String, BusError> {
    let start = format!("-{lines}");
    let output = std::process::Command::new("tmux")
        .args(["capture-pane", "-t", tmux_target, "-p", "-S", &start])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(BusError::TmuxCaptureFailed(
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
        ))
    }
}

fn probe_tmux_target(tmux_target: &str) -> TmuxProbeReport {
    match std::process::Command::new("tmux")
        .args(["has-session", "-t", tmux_target])
        .status()
    {
        Ok(status) if status.success() => TmuxProbeReport {
            agent_id: String::new(),
            tmux_target: tmux_target.to_string(),
            status: "ok".to_string(),
            detail: None,
        },
        Ok(status) => TmuxProbeReport {
            agent_id: String::new(),
            tmux_target: tmux_target.to_string(),
            status: "failed".to_string(),
            detail: Some(
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "terminated by signal".to_string()),
            ),
        },
        Err(error) => TmuxProbeReport {
            agent_id: String::new(),
            tmux_target: tmux_target.to_string(),
            status: "failed".to_string(),
            detail: Some(error.to_string()),
        },
    }
}

fn capture_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let seq = BUS_EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cowork-capture-{nanos}-{seq}")
}

pub fn drain_agent(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
) -> Result<Vec<InboxMessage>, BusError> {
    validate_agent_id(agent_id)?;
    let registry = load_registry(mempal_home, cwd)?;
    if !registry.agents.contains_key(agent_id) {
        return Err(BusError::UnknownAgent(agent_id.to_string()));
    }

    let path = agent_inbox_path(mempal_home, cwd, agent_id)?;
    let draining = path.with_extension("draining");
    match std::fs::rename(&path, &draining) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            append_drain_event(mempal_home, cwd, agent_id, 0, 0)?;
            return Ok(Vec::new());
        }
        Err(e) => return Err(e.into()),
    }

    let raw = std::fs::read_to_string(&draining)?;
    let mut messages = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(msg) = serde_json::from_str::<InboxMessage>(trimmed) {
            messages.push(msg);
        }
    }
    let _ = std::fs::remove_file(&draining);
    append_drain_event(mempal_home, cwd, agent_id, messages.len(), raw.len() as u64)?;
    Ok(messages)
}

pub fn list_events(
    mempal_home: &Path,
    cwd: &Path,
    limit: Option<usize>,
) -> Result<Vec<BusEvent>, BusError> {
    let path = events_path(mempal_home, cwd)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = std::fs::read_to_string(path)?;
    let mut events = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        events.push(serde_json::from_str::<BusEvent>(trimmed)?);
    }

    if let Some(limit) = limit
        && events.len() > limit
    {
        events.drain(0..events.len() - limit);
    }
    Ok(events)
}

pub fn list_delivery_statuses(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: Option<&str>,
) -> Result<Vec<DeliveryStatus>, BusError> {
    if let Some(agent_id) = agent_id {
        validate_agent_id(agent_id)?;
    }
    let events = list_events(mempal_home, cwd, None)?;
    let mut statuses = Vec::<DeliveryStatus>::new();
    let mut index_by_message_id = BTreeMap::<String, usize>::new();

    for event in events {
        match event.event_type.as_str() {
            "send" | "broadcast" if event.status == "delivered" || event.status == "failed" => {
                let from = event.actor_agent_id.clone().unwrap_or_default();
                let transport = event.transport.clone().unwrap_or_default();
                for target in &event.target_agent_ids {
                    let status = if event.status == "failed" {
                        "failed"
                    } else {
                        "pending"
                    };
                    let delivery = DeliveryStatus {
                        message_id: event.event_id.clone(),
                        event_type: event.event_type.clone(),
                        status: status.to_string(),
                        from: from.clone(),
                        target_agent_id: target.clone(),
                        transport: transport.clone(),
                        message_preview: event.message_preview.clone(),
                        thread_id: event.details.get("thread_id").cloned(),
                        channel: event.details.get("channel").cloned(),
                        delivered_at: event.occurred_at.clone(),
                        updated_at: event.occurred_at.clone(),
                        acked_by: None,
                    };
                    index_by_message_id.insert(delivery.message_id.clone(), statuses.len());
                    statuses.push(delivery);
                }
            }
            "drain" if event.status == "drained" => {
                let Some(target) = event.target_agent_ids.first() else {
                    continue;
                };
                let drained_count = event
                    .details
                    .get("drained_count")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or_default();
                if drained_count == 0 {
                    continue;
                }
                let mut remaining = drained_count;
                for status in statuses.iter_mut() {
                    if remaining == 0 {
                        break;
                    }
                    if status.target_agent_id == *target && status.status == "pending" {
                        status.status = "drained".to_string();
                        status.updated_at = event.occurred_at.clone();
                        remaining -= 1;
                    }
                }
            }
            "ack" if event.status == "acked" => {
                let Some(message_id) = event.details.get("delivery_event_id") else {
                    continue;
                };
                let Some(index) = index_by_message_id.get(message_id).copied() else {
                    continue;
                };
                let status = &mut statuses[index];
                status.status = "acked".to_string();
                status.updated_at = event.occurred_at.clone();
                status.acked_by = event.actor_agent_id.clone();
            }
            _ => {}
        }
    }

    if let Some(agent_id) = agent_id {
        statuses.retain(|status| status.target_agent_id == agent_id);
    }
    Ok(statuses)
}

pub fn ack_delivery(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
    message_id: &str,
) -> Result<DeliveryStatus, BusError> {
    validate_agent_id(agent_id)?;
    let registry = load_registry(mempal_home, cwd)?;
    if !registry.agents.contains_key(agent_id) {
        return Err(BusError::UnknownAgent(agent_id.to_string()));
    }

    let deliveries = list_delivery_statuses(mempal_home, cwd, None)?;
    let delivery = deliveries
        .iter()
        .find(|status| status.message_id == message_id)
        .ok_or_else(|| BusError::UnknownDelivery(message_id.to_string()))?;
    if delivery.target_agent_id != agent_id {
        return Err(BusError::DeliveryTargetMismatch {
            message_id: message_id.to_string(),
            target_agent_id: delivery.target_agent_id.clone(),
            agent_id: agent_id.to_string(),
        });
    }
    if delivery.status == "failed" {
        return Err(BusError::CannotAckFailed(message_id.to_string()));
    }
    if delivery.status != "acked" {
        let mut details = BTreeMap::new();
        details.insert("delivery_event_id".to_string(), message_id.to_string());
        append_bus_event(
            mempal_home,
            cwd,
            "ack",
            "acked",
            Some(agent_id.to_string()),
            vec![agent_id.to_string()],
            Some(delivery.transport.clone()),
            None,
            details,
        )?;
    }

    list_delivery_statuses(mempal_home, cwd, Some(agent_id))?
        .into_iter()
        .find(|status| status.message_id == message_id)
        .ok_or_else(|| BusError::UnknownDelivery(message_id.to_string()))
}

pub fn format_agent_plain(agent_id: &str, messages: &[InboxMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "[Multi-agent cowork inbox for {} ({} message{} since last check):]\n",
        agent_id,
        messages.len(),
        if messages.len() == 1 { "" } else { "s" }
    );
    for msg in messages {
        let mut metadata = Vec::new();
        if let Some(thread_id) = &msg.thread_id {
            metadata.push(format!("thread={thread_id}"));
        }
        if let Some(channel) = &msg.channel {
            metadata.push(format!("channel={channel}"));
        }
        let metadata = if metadata.is_empty() {
            String::new()
        } else {
            format!(" [{}]", metadata.join(" "))
        };
        out.push_str(&format!(
            "- {} from {}{}: {}\n",
            msg.pushed_at, msg.from, metadata, msg.content
        ));
    }
    out.push_str("[End multi-agent cowork inbox]\n");
    out
}

pub fn format_events_plain(events: &[BusEvent]) -> String {
    let mut out = String::new();
    for event in events {
        let actor = event.actor_agent_id.as_deref().unwrap_or("-");
        let targets = if event.target_agent_ids.is_empty() {
            "-".to_string()
        } else {
            event.target_agent_ids.join(",")
        };
        let transport = event.transport.as_deref().unwrap_or("-");
        let preview = event.message_preview.as_deref().unwrap_or("-");
        let details = format_details(&event.details);
        out.push_str(&format!(
            "{} {} type={} status={} actor={} targets={} transport={} preview={} details={}\n",
            event.event_id,
            event.occurred_at,
            event.event_type,
            event.status,
            actor,
            targets,
            transport,
            preview,
            details
        ));
    }
    out
}

pub fn format_delivery_statuses_plain(deliveries: &[DeliveryStatus]) -> String {
    let mut out = String::new();
    for delivery in deliveries {
        let preview = delivery.message_preview.as_deref().unwrap_or("-");
        let thread_id = delivery.thread_id.as_deref().unwrap_or("-");
        let channel = delivery.channel.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "{} {} status={} from={} target={} transport={} thread={} channel={} preview={}\n",
            delivery.message_id,
            delivery.updated_at,
            delivery.status,
            delivery.from,
            delivery.target_agent_id,
            delivery.transport,
            thread_id,
            channel,
            preview
        ));
    }
    out
}

pub fn format_doctor_plain(report: &DoctorReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "status={} agents={} channels={} sessions={} stale_agents={} never_seen_agents={} pending_deliveries={}\n",
        report.status,
        report.agent_count,
        report.channel_count,
        report.session_count,
        report.stale_agents,
        report.never_seen_agents,
        report.pending_deliveries
    ));
    for warning in &report.warnings {
        out.push_str(&format!("warning: {warning}\n"));
    }
    for probe in &report.tmux {
        out.push_str(&format!(
            "tmux agent={} target={} status={} detail={}\n",
            probe.agent_id,
            probe.tmux_target,
            probe.status,
            probe.detail.as_deref().unwrap_or("-")
        ));
    }
    out
}

pub fn format_sessions_plain(sessions: &[TeamSession]) -> String {
    if sessions.is_empty() {
        return "no sessions\n".to_string();
    }
    let mut out = String::new();
    for session in sessions {
        out.push_str(&format!(
            "{} status={} title={} agents={} channels={} thread={} updated_at={}\n",
            session.session_id,
            session.status,
            session.title,
            session.agents.join(","),
            if session.channels.is_empty() {
                "-".to_string()
            } else {
                session.channels.join(",")
            },
            session.thread_id.as_deref().unwrap_or("-"),
            session.updated_at
        ));
        if let Some(goal) = &session.goal {
            out.push_str(&format!("  goal={goal}\n"));
        }
    }
    out
}

pub fn format_handoff_plain(summary: &HandoffSummary) -> String {
    let mut out = String::new();
    out.push_str("Cowork Handoff Summary\n");
    out.push_str(&format!(
        "filters thread={} channel={} session={} limit={}\n\n",
        summary.filters.thread_id.as_deref().unwrap_or("-"),
        summary.filters.channel.as_deref().unwrap_or("-"),
        summary.filters.session_id.as_deref().unwrap_or("-"),
        summary.filters.limit
    ));

    out.push_str("Active sessions\n");
    let active_sessions = summary
        .sessions
        .iter()
        .filter(|session| session.status == "active")
        .collect::<Vec<_>>();
    if active_sessions.is_empty() {
        out.push_str("- none\n");
    } else {
        for session in active_sessions {
            out.push_str(&format!(
                "- {} [{}] agents={} thread={} goal={}\n",
                session.session_id,
                session.title,
                session.agents.join(","),
                session.thread_id.as_deref().unwrap_or("-"),
                session.goal.as_deref().unwrap_or("-")
            ));
        }
    }

    out.push_str("\nAgents\n");
    if summary.agents.is_empty() {
        out.push_str("- none\n");
    } else {
        for agent in &summary.agents {
            out.push_str(&format!(
                "- {} tool={} presence={} pending={}\n",
                agent.agent_id, agent.tool, agent.presence, agent.pending_count
            ));
        }
    }

    out.push_str("\nPending deliveries\n");
    if summary.pending_deliveries.is_empty() {
        out.push_str("- none\n");
    } else {
        for delivery in &summary.pending_deliveries {
            out.push_str(&format!(
                "- {} from={} target={} thread={} channel={} preview={}\n",
                delivery.message_id,
                delivery.from,
                delivery.target_agent_id,
                delivery.thread_id.as_deref().unwrap_or("-"),
                delivery.channel.as_deref().unwrap_or("-"),
                delivery.message_preview.as_deref().unwrap_or("-")
            ));
        }
    }

    out.push_str("\nRecent events\n");
    if summary.recent_events.is_empty() {
        out.push_str("- none\n");
    } else {
        for event in &summary.recent_events {
            out.push_str(&format!(
                "- {} type={} status={} actor={} targets={} details={}\n",
                event.event_id,
                event.event_type,
                event.status,
                event.actor_agent_id.as_deref().unwrap_or("-"),
                event.target_agent_ids.join(","),
                format_details(&event.details)
            ));
        }
    }
    out
}

pub fn format_capture_plain(report: &CoworkCaptureReport) -> String {
    format!(
        "writes={} drawer_id={} wing={} room={} source={}\n",
        report.writes,
        report.drawer_id.as_deref().unwrap_or("-"),
        report.wing,
        report.room.as_deref().unwrap_or("-"),
        report.source
    )
}

fn append_drain_event(
    mempal_home: &Path,
    cwd: &Path,
    agent_id: &str,
    drained_count: usize,
    drained_bytes: u64,
) -> Result<BusEvent, BusError> {
    let mut details = BTreeMap::new();
    details.insert("drained_count".to_string(), drained_count.to_string());
    details.insert("drained_bytes".to_string(), drained_bytes.to_string());
    append_bus_event(
        mempal_home,
        cwd,
        "drain",
        "drained",
        Some(agent_id.to_string()),
        vec![agent_id.to_string()],
        Some("inbox".to_string()),
        None,
        details,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_bus_event(
    mempal_home: &Path,
    cwd: &Path,
    event_type: &str,
    status: &str,
    actor_agent_id: Option<String>,
    target_agent_ids: Vec<String>,
    transport: Option<String>,
    message_preview: Option<String>,
    details: BTreeMap<String, String>,
) -> Result<BusEvent, BusError> {
    use std::io::Write;

    let event = BusEvent {
        event_id: new_event_id(),
        occurred_at: format_rfc3339(SystemTime::now()),
        event_type: event_type.to_string(),
        status: status.to_string(),
        actor_agent_id,
        target_agent_ids,
        transport,
        message_preview,
        details,
    };
    let path = events_path(mempal_home, cwd)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(&event)?)?;
    file.flush()?;
    Ok(event)
}

fn new_event_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let seq = BUS_EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("evt-{nanos}-{seq}")
}

fn message_preview(content: &str) -> String {
    let mut preview: String = content
        .chars()
        .take(MAX_EVENT_MESSAGE_PREVIEW_CHARS)
        .collect();
    if content.chars().count() > MAX_EVENT_MESSAGE_PREVIEW_CHARS {
        preview.push_str("...");
    }
    preview
}

fn add_optional_detail(details: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        details.insert(key.to_string(), value.to_string());
    }
}

fn format_details(details: &BTreeMap<String, String>) -> String {
    if details.is_empty() {
        return "-".to_string();
    }
    details
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn presence_for(last_seen_at: Option<&str>, now_seconds: i64) -> String {
    let Some(last_seen_at) = last_seen_at else {
        return "never_seen".to_string();
    };
    let Some(last_seen_seconds) = parse_rfc3339(last_seen_at) else {
        return "stale".to_string();
    };
    if now_seconds.saturating_sub(last_seen_seconds) <= PRESENCE_STALE_AFTER_SECONDS {
        "online".to_string()
    } else {
        "stale".to_string()
    }
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn read_inbox_stats(path: &Path) -> Result<(usize, u64, Vec<InboxMessage>), BusError> {
    if !path.exists() {
        return Ok((0, 0, Vec::new()));
    }
    let raw = std::fs::read_to_string(path)?;
    let mut count = 0;
    let mut preview = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        count += 1;
        if preview.len() < 3 {
            if let Ok(msg) = serde_json::from_str::<InboxMessage>(trimmed) {
                preview.push(msg);
            }
        }
    }
    Ok((count, raw.len() as u64, preview))
}
