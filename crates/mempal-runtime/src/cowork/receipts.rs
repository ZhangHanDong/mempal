//! Delivery receipts for the P8 cowork pair-push channel (P116).
//!
//! Append-only observability log answering "did the partner ever see that
//! handoff?" (GitHub issue #81). `push` records a `queued` event, `drain`
//! records one `drained` event per message; message state (pending /
//! drained / lost) is derived by joining events with the live inbox —
//! nothing is stored in palace.db and delivery semantics are unchanged.
//!
//! Spec: specs/p116-cowork-pair-delivery-receipts.spec.md

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::inbox::{self, InboxError};
use super::peek::Tool;

/// Keep at most this many receipt events per project file (newest win).
pub const MAX_RECEIPT_EVENTS: usize = 400;

pub const EVENT_QUEUED: &str = "queued";
pub const EVENT_DRAINED: &str = "drained";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptEvent {
    /// "queued" or "drained".
    pub event: String,
    /// Absent for pre-P116 inbox lines drained after upgrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub from: String,
    pub to: String,
    /// RFC3339 timestamp of the event itself.
    pub at: String,
    /// Drain output format actually injected ("plain" / "codex-hook-json").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injected_as: Option<String>,
    /// Optional caller-supplied hook runtime label (`--hook-runtime`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_runtime: Option<String>,
}

/// Metadata a draining caller supplies so the receipt records how the
/// messages were injected.
#[derive(Debug, Clone)]
pub struct DrainMeta {
    pub injected_as: String,
    pub hook_runtime: Option<String>,
    pub drained_at: String,
}

/// Derived per-message delivery state.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MessageReceiptState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub from: String,
    pub to: String,
    pub queued_at: Option<String>,
    pub drained_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected_as: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_runtime: Option<String>,
    /// "pending" | "drained" | "lost".
    pub status: String,
}

/// `<mempal_home>/cowork-inbox/receipts/<encoded_project_identity>.jsonl`.
pub fn receipts_path(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, InboxError> {
    let identity = inbox::project_identity(cwd);
    let encoded = inbox::encode_project_identity(&identity)?;
    Ok(mempal_home
        .join("cowork-inbox")
        .join("receipts")
        .join(format!("{encoded}.jsonl")))
}

/// Append one event, rotating in place so the file never holds more than
/// MAX_RECEIPT_EVENTS (newest win).
pub fn append_event(
    mempal_home: &Path,
    cwd: &Path,
    event: &ReceiptEvent,
) -> Result<(), InboxError> {
    use std::fs;
    use std::io::Write;

    let path = receipts_path(mempal_home, cwd)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut events = load_events(mempal_home, cwd)?;
    events.push(event.clone());

    if events.len() > MAX_RECEIPT_EVENTS {
        let keep_from = events.len() - MAX_RECEIPT_EVENTS;
        let mut buffer = String::new();
        for kept in &events[keep_from..] {
            buffer.push_str(&serde_json::to_string(kept)?);
            buffer.push('\n');
        }
        fs::write(&path, buffer)?;
    } else {
        let line = serde_json::to_string(event)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(file, "{line}")?;
        file.flush()?;
    }
    Ok(())
}

/// Load all events for this project identity (oldest first). Missing file
/// yields an empty Vec; malformed lines are skipped.
pub fn load_events(mempal_home: &Path, cwd: &Path) -> Result<Vec<ReceiptEvent>, InboxError> {
    let path = receipts_path(mempal_home, cwd)?;
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let mut events = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<ReceiptEvent>(trimmed) {
            events.push(event);
        }
    }
    Ok(events)
}

/// Join receipt events with the live inbox files into per-message states,
/// newest queued_at first.
pub fn message_states(
    mempal_home: &Path,
    cwd: &Path,
) -> Result<Vec<MessageReceiptState>, InboxError> {
    use std::collections::{HashMap, HashSet};

    let events = load_events(mempal_home, cwd)?;

    // Receipt handles still sitting in a live inbox file are pending.
    let mut in_flight: HashSet<String> = HashSet::new();
    for target in [Tool::Claude, Tool::Codex] {
        let path = inbox::inbox_path(mempal_home, target, cwd)?;
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            if let Ok(message) = serde_json::from_str::<inbox::InboxMessage>(line.trim())
                && let Some(id) = message.message_id
            {
                in_flight.insert(id);
            }
        }
    }

    let mut drained_by_id: HashMap<String, &ReceiptEvent> = HashMap::new();
    let mut orphan_drained: Vec<&ReceiptEvent> = Vec::new();
    for event in events.iter().filter(|e| e.event == EVENT_DRAINED) {
        match &event.message_id {
            Some(id) => {
                drained_by_id.insert(id.clone(), event);
            }
            None => orphan_drained.push(event),
        }
    }

    let mut states = Vec::new();
    for queued in events.iter().filter(|e| e.event == EVENT_QUEUED) {
        let drained = queued
            .message_id
            .as_ref()
            .and_then(|id| drained_by_id.remove(id));
        let status = if drained.is_some() {
            "drained"
        } else if queued
            .message_id
            .as_ref()
            .is_some_and(|id| in_flight.contains(id))
        {
            "pending"
        } else {
            "lost"
        };
        states.push(MessageReceiptState {
            message_id: queued.message_id.clone(),
            from: queued.from.clone(),
            to: queued.to.clone(),
            queued_at: Some(queued.at.clone()),
            drained_at: drained.map(|e| e.at.clone()),
            injected_as: drained.and_then(|e| e.injected_as.clone()),
            hook_runtime: drained.and_then(|e| e.hook_runtime.clone()),
            status: status.to_string(),
        });
    }

    // Drained events with no queued counterpart (legacy lines, rotated-away
    // queued events) still deserve a row.
    for event in orphan_drained
        .into_iter()
        .chain(drained_by_id.into_values())
    {
        states.push(MessageReceiptState {
            message_id: event.message_id.clone(),
            from: event.from.clone(),
            to: event.to.clone(),
            queued_at: None,
            drained_at: Some(event.at.clone()),
            injected_as: event.injected_as.clone(),
            hook_runtime: event.hook_runtime.clone(),
            status: "drained".to_string(),
        });
    }

    // Newest queued_at first; rows without queued_at sink to the end.
    states.sort_by(|a, b| b.queued_at.cmp(&a.queued_at));
    Ok(states)
}

/// Drain the pair inbox exactly like `inbox::drain`, then best-effort append
/// one `drained` receipt event per message using `meta`.
pub fn drain_with_receipt(
    mempal_home: &Path,
    target: Tool,
    cwd: &Path,
    meta: &DrainMeta,
) -> Result<Vec<inbox::InboxMessage>, InboxError> {
    let messages = inbox::drain(mempal_home, target, cwd)?;
    for message in &messages {
        let event = ReceiptEvent {
            event: EVENT_DRAINED.to_string(),
            message_id: message.message_id.clone(),
            from: message.from.clone(),
            to: target.dir_name().to_string(),
            at: meta.drained_at.clone(),
            injected_as: Some(meta.injected_as.clone()),
            hook_runtime: meta.hook_runtime.clone(),
        };
        // Receipts are observability; never fail the drain over them.
        let _ = append_event(mempal_home, cwd, &event);
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmpdir_with_git() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("proj");
        fs::create_dir_all(repo.join(".git")).unwrap();
        (tmp, repo)
    }

    fn rfc3339(n: u64) -> String {
        format!("2026-07-26T00:00:{n:02}Z")
    }

    #[test]
    fn message_id_is_deterministic_and_prefixed() {
        let a = inbox::build_message_id("2026-07-26T00:00:00Z", "claude", "hello");
        let b = inbox::build_message_id("2026-07-26T00:00:00Z", "claude", "hello");
        let c = inbox::build_message_id("2026-07-26T00:00:00Z", "claude", "other");

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("msg_"), "unexpected id shape: {a}");
        assert_eq!(a.len(), "msg_".len() + 12);
    }

    #[test]
    fn push_returns_message_id_and_appends_queued_event() {
        let (tmp_home, repo) = tmpdir_with_git();

        let outcome = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "handoff body".to_string(),
            rfc3339(0),
        )
        .unwrap();

        assert!(outcome.message_id.starts_with("msg_"));
        assert!(outcome.inbox_size_after > 0);

        let events = load_events(tmp_home.path(), &repo).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, EVENT_QUEUED);
        assert_eq!(
            events[0].message_id.as_deref(),
            Some(outcome.message_id.as_str())
        );
        assert_eq!(events[0].from, "claude");
        assert_eq!(events[0].to, "codex");
        assert_eq!(events[0].at, rfc3339(0));
    }

    #[test]
    fn drain_with_receipt_appends_drained_events_with_meta() {
        let (tmp_home, repo) = tmpdir_with_git();
        let outcome = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "one".to_string(),
            rfc3339(0),
        )
        .unwrap();

        let meta = DrainMeta {
            injected_as: "codex-hook-json".to_string(),
            hook_runtime: Some("codex UserPromptSubmit".to_string()),
            drained_at: rfc3339(5),
        };
        let messages = drain_with_receipt(tmp_home.path(), Tool::Codex, &repo, &meta).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].message_id.as_deref(),
            Some(outcome.message_id.as_str())
        );

        let events = load_events(tmp_home.path(), &repo).unwrap();
        let drained: Vec<_> = events.iter().filter(|e| e.event == EVENT_DRAINED).collect();
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0].message_id.as_deref(),
            Some(outcome.message_id.as_str())
        );
        assert_eq!(drained[0].injected_as.as_deref(), Some("codex-hook-json"));
        assert_eq!(
            drained[0].hook_runtime.as_deref(),
            Some("codex UserPromptSubmit")
        );
        assert_eq!(drained[0].at, rfc3339(5));
    }

    #[test]
    fn message_states_derives_pending_drained_and_lost() {
        let (tmp_home, repo) = tmpdir_with_git();

        // pending: queued, still in inbox
        let pending = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "pending message".to_string(),
            rfc3339(0),
        )
        .unwrap();

        // drained: queued to the OTHER target, then drained
        let drained = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Codex,
            Tool::Claude,
            &repo,
            "drained message".to_string(),
            rfc3339(1),
        )
        .unwrap();
        let meta = DrainMeta {
            injected_as: "plain".to_string(),
            hook_runtime: None,
            drained_at: rfc3339(2),
        };
        drain_with_receipt(tmp_home.path(), Tool::Claude, &repo, &meta).unwrap();

        // lost: queued, then inbox file vanishes without a drained event
        let lost = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "lost message".to_string(),
            rfc3339(3),
        )
        .unwrap();
        // simulate the drain crash window: rename happened, receipt never written
        let codex_inbox = inbox::inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        fs::remove_file(&codex_inbox).unwrap();
        // note: this also removes `pending`'s line — re-push it to restore
        let pending2 = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "pending message".to_string(),
            rfc3339(4),
        )
        .unwrap();

        let states = message_states(tmp_home.path(), &repo).unwrap();
        let by_id = |id: &str| {
            states
                .iter()
                .find(|s| s.message_id.as_deref() == Some(id))
                .unwrap_or_else(|| panic!("state for {id} missing: {states:?}"))
        };

        assert_eq!(by_id(&pending2.message_id).status, "pending");
        assert_eq!(by_id(&drained.message_id).status, "drained");
        assert_eq!(
            by_id(&drained.message_id).injected_as.as_deref(),
            Some("plain")
        );
        assert_eq!(by_id(&lost.message_id).status, "lost");
        // first `pending` push shares content+from but has an earlier
        // timestamp, so it is a distinct id that also became lost
        assert_eq!(by_id(&pending.message_id).status, "lost");

        // newest queued_at first
        let queued_order: Vec<_> = states.iter().filter_map(|s| s.queued_at.clone()).collect();
        let mut sorted = queued_order.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(queued_order, sorted);
    }

    #[test]
    fn receipts_rotation_keeps_newest_events_under_cap() {
        let (tmp_home, repo) = tmpdir_with_git();

        for i in 0..(MAX_RECEIPT_EVENTS + 25) {
            let event = ReceiptEvent {
                event: EVENT_QUEUED.to_string(),
                message_id: Some(format!("msg_{i:012}")),
                from: "claude".to_string(),
                to: "codex".to_string(),
                at: format!("2026-07-26T01:{:02}:{:02}Z", i / 60, i % 60),
                injected_as: None,
                hook_runtime: None,
            };
            append_event(tmp_home.path(), &repo, &event).unwrap();
        }

        let events = load_events(tmp_home.path(), &repo).unwrap();
        assert_eq!(events.len(), MAX_RECEIPT_EVENTS);
        // the newest event must be the last one appended
        assert_eq!(
            events.last().unwrap().message_id.as_deref(),
            Some(format!("msg_{:012}", MAX_RECEIPT_EVENTS + 24).as_str())
        );
    }

    #[test]
    fn legacy_inbox_line_without_message_id_still_drains_with_receipt() {
        let (tmp_home, repo) = tmpdir_with_git();

        // hand-write a pre-P116 inbox line (no message_id field)
        let path = inbox::inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "{\"pushed_at\":\"2026-07-01T00:00:00Z\",\"from\":\"claude\",\"content\":\"old style\"}\n",
        )
        .unwrap();

        let meta = DrainMeta {
            injected_as: "plain".to_string(),
            hook_runtime: None,
            drained_at: rfc3339(9),
        };
        let messages = drain_with_receipt(tmp_home.path(), Tool::Codex, &repo, &meta).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "old style");
        assert_eq!(messages[0].message_id, None);

        let events = load_events(tmp_home.path(), &repo).unwrap();
        let drained: Vec<_> = events.iter().filter(|e| e.event == EVENT_DRAINED).collect();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].message_id, None);
    }
}
