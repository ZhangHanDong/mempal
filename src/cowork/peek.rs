//! Peek request/response types + orchestration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::cowork::claude::{claude_project_dir, latest_session_file, parse_jsonl_messages};
use crate::cowork::codex::{find_latest_session_for_cwd, parse_codex_jsonl};

/// Which agent tool's session to peek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
    Auto,
}

impl Tool {
    /// Case-insensitive parse from a string; used for ClientInfo.name matching.
    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "claude_code" => Some(Tool::Claude),
            "codex" | "codex-cli" | "codex_cli" | "codex-tui" => Some(Tool::Codex),
            "auto" => Some(Tool::Auto),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Auto => "auto",
        }
    }
}

/// Peek request — parameters to `peek_partner`.
#[derive(Debug, Clone)]
pub struct PeekRequest {
    pub tool: Tool,
    /// Max messages to return (default 30).
    pub limit: usize,
    /// Optional RFC3339 cutoff; only messages newer than this are returned.
    pub since: Option<String>,
    /// Absolute cwd of the caller (injected by orchestrator; not user-facing).
    pub cwd: PathBuf,
    /// The tool that the CALLER is; used to reject self-peek.
    /// `None` means unknown (ClientInfo missing); auto mode will then error.
    pub caller_tool: Option<Tool>,
    /// HOME override for tests. None = use $HOME env var.
    pub home_override: Option<PathBuf>,
}

/// A single message from a session log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeekMessage {
    /// "user" or "assistant".
    pub role: String,
    /// RFC3339 timestamp of this message.
    pub at: String,
    /// Plain text content; tool-use internals are filtered out.
    pub text: String,
}

/// Peek response — what `peek_partner` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeekResponse {
    pub partner_tool: Tool,
    pub session_path: Option<String>,
    pub session_mtime: Option<String>,
    pub partner_active: bool,
    pub messages: Vec<PeekMessage>,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum PeekError {
    #[error(
        "cannot infer partner; pass `tool` explicitly (client_info.name was missing or unrecognized)"
    )]
    CannotInferPartner,

    #[error("cannot peek your own session")]
    SelfPeek,

    #[error("I/O error reading session: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse session file: {0}")]
    Parse(String),
}

/// A partner session is "active" if its mtime is within 30 minutes.
const ACTIVE_WINDOW: Duration = Duration::from_secs(30 * 60);

/// Check whether a given mtime falls inside the active window.
/// Future mtimes (clock skew) are treated as active.
pub fn is_active(mtime: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(mtime)
        .map(|d| d <= ACTIVE_WINDOW)
        .unwrap_or(true)
}

/// Resolve `Tool::Auto` into a concrete partner tool based on caller identity.
pub fn infer_partner(requested: Tool, caller_tool: Option<Tool>) -> Result<Tool, PeekError> {
    match requested {
        Tool::Claude | Tool::Codex => Ok(requested),
        Tool::Auto => match caller_tool {
            Some(Tool::Claude) => Ok(Tool::Codex),
            Some(Tool::Codex) => Ok(Tool::Claude),
            _ => Err(PeekError::CannotInferPartner),
        },
    }
}

/// Format a SystemTime as RFC3339 UTC (seconds precision).
pub fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let sec_of_day = secs % 86400;
    let hour = sec_of_day / 3600;
    let minute = (sec_of_day / 60) % 60;
    let second = sec_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's civil_from_days — convert days since 1970-01-01 to
/// (year, month, day) in the proleptic Gregorian calendar.
fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

fn resolve_home(request: &PeekRequest) -> Result<PathBuf, PeekError> {
    if let Some(h) = &request.home_override {
        return Ok(h.clone());
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| PeekError::Parse("HOME environment variable not set".to_string()))
}

/// Main entry point: dispatch to the correct adapter based on target tool.
pub fn peek_partner(request: PeekRequest) -> Result<PeekResponse, PeekError> {
    let target = infer_partner(request.tool, request.caller_tool)?;

    if let Some(caller) = request.caller_tool {
        if caller == target {
            return Err(PeekError::SelfPeek);
        }
    }

    match target {
        Tool::Claude => peek_claude(&request, target),
        Tool::Codex => peek_codex(&request, target),
        Tool::Auto => unreachable!("infer_partner should have resolved Auto"),
    }
}

fn peek_claude(request: &PeekRequest, target: Tool) -> Result<PeekResponse, PeekError> {
    let home = resolve_home(request)?;
    let project_dir = claude_project_dir(&home, &request.cwd);
    let Some((path, mtime)) = latest_session_file(&project_dir) else {
        return Ok(empty_response(target));
    };

    let (messages, truncated) =
        parse_jsonl_messages(&path, request.since.as_deref(), request.limit)?;

    Ok(PeekResponse {
        partner_tool: target,
        session_path: Some(path.to_string_lossy().into_owned()),
        session_mtime: Some(format_rfc3339(mtime)),
        partner_active: is_active(mtime),
        messages,
        truncated,
    })
}

fn peek_codex(request: &PeekRequest, target: Tool) -> Result<PeekResponse, PeekError> {
    let home = resolve_home(request)?;
    let base = home.join(".codex/sessions");
    let target_cwd = request.cwd.to_string_lossy().into_owned();
    let Some((path, mtime)) = find_latest_session_for_cwd(&base, &target_cwd)? else {
        return Ok(empty_response(target));
    };

    let (messages, truncated) =
        parse_codex_jsonl(&path, request.since.as_deref(), request.limit)?;

    Ok(PeekResponse {
        partner_tool: target,
        session_path: Some(path.to_string_lossy().into_owned()),
        session_mtime: Some(format_rfc3339(mtime)),
        partner_active: is_active(mtime),
        messages,
        truncated,
    })
}

fn empty_response(target: Tool) -> PeekResponse {
    PeekResponse {
        partner_tool: target,
        session_path: None,
        session_mtime: None,
        partner_active: false,
        messages: vec![],
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_parses_from_str() {
        assert_eq!(Tool::from_str_ci("claude"), Some(Tool::Claude));
        assert_eq!(Tool::from_str_ci("Codex"), Some(Tool::Codex));
        assert_eq!(Tool::from_str_ci("AUTO"), Some(Tool::Auto));
        assert_eq!(Tool::from_str_ci("other"), None);
    }

    #[test]
    fn tool_parses_compound_names() {
        assert_eq!(Tool::from_str_ci("claude-code"), Some(Tool::Claude));
        assert_eq!(Tool::from_str_ci("codex-cli"), Some(Tool::Codex));
        assert_eq!(Tool::from_str_ci("codex-tui"), Some(Tool::Codex));
    }

    #[test]
    fn rejects_self_peek_when_caller_is_same_tool() {
        let req = PeekRequest {
            tool: Tool::Codex,
            limit: 30,
            since: None,
            cwd: std::path::PathBuf::from("/tmp"),
            caller_tool: Some(Tool::Codex),
            home_override: None,
        };
        let err = peek_partner(req).unwrap_err();
        assert!(matches!(err, PeekError::SelfPeek));
    }

    #[test]
    fn auto_mode_errors_without_caller_tool() {
        let req = PeekRequest {
            tool: Tool::Auto,
            limit: 30,
            since: None,
            cwd: std::path::PathBuf::from("/tmp"),
            caller_tool: None,
            home_override: None,
        };
        let err = peek_partner(req).unwrap_err();
        assert!(matches!(err, PeekError::CannotInferPartner));
    }

    #[test]
    fn infer_partner_maps_claude_to_codex_and_vice_versa() {
        assert_eq!(
            infer_partner(Tool::Auto, Some(Tool::Claude)).unwrap(),
            Tool::Codex
        );
        assert_eq!(
            infer_partner(Tool::Auto, Some(Tool::Codex)).unwrap(),
            Tool::Claude
        );
        assert_eq!(
            infer_partner(Tool::Claude, Some(Tool::Codex)).unwrap(),
            Tool::Claude
        );
    }

    #[test]
    fn is_active_true_when_mtime_within_30_minutes() {
        use std::time::{Duration, SystemTime};
        let recent = SystemTime::now() - Duration::from_secs(10 * 60);
        let old = SystemTime::now() - Duration::from_secs(45 * 60);
        assert!(is_active(recent));
        assert!(!is_active(old));
    }

    #[test]
    fn peek_response_serializes_with_snake_case_fields() {
        let resp = PeekResponse {
            partner_tool: Tool::Codex,
            session_path: Some("/tmp/x.jsonl".into()),
            session_mtime: Some("2026-04-13T12:00:00Z".into()),
            partner_active: true,
            messages: vec![],
            truncated: false,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("partner_tool"));
        assert!(json.contains("session_path"));
        assert!(json.contains("partner_active"));
        assert!(json.contains(r#""partner_tool":"codex""#));
    }
}
