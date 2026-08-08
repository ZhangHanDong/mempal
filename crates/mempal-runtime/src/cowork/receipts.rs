//! Delivery receipts for the P8 cowork pair-push channel (P116).
//!
//! Bounded observability event log answering "did the partner ever see that
//! handoff?" (GitHub issue #81). `push` records a `queued` event; after
//! successful injection, the drain caller records one `drained` event per
//! message. Message state (pending / drained / lost) is derived by joining
//! events with the live inbox — nothing is stored in palace.db and delivery
//! semantics are unchanged.
//!
//! Spec: specs/p116-cowork-pair-delivery-receipts.spec.md

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
///
/// # Errors
///
/// Returns [`InboxError`] when `cwd` cannot be encoded as a valid project
/// identity.
pub fn receipts_path(mempal_home: &Path, cwd: &Path) -> Result<PathBuf, InboxError> {
    let identity = inbox::project_identity(cwd);
    let encoded = inbox::encode_project_identity(&identity)?;
    Ok(mempal_home
        .join("cowork-inbox")
        .join("receipts")
        .join(format!("{encoded}.jsonl")))
}

/// Append one event, rotating in place so the file never holds more than
/// [`MAX_RECEIPT_EVENTS`] (newest win).
///
/// On Unix, the load-decide-append/rewrite section is serialized with the P9
/// flock so concurrent push/drain cannot exceed the cap or lose a fresh event
/// to a racing rewrite. Windows retains P9's documented no-op; if effective
/// locking is unavailable, the append proceeds unlocked — delivery
/// observability beats strict concurrent rotation.
///
/// # Errors
///
/// Returns [`InboxError`] when the receipt path cannot be encoded or the
/// receipt log cannot be read, serialized, or written.
pub fn append_event(
    mempal_home: &Path,
    cwd: &Path,
    event: &ReceiptEvent,
) -> Result<(), InboxError> {
    let _guard = acquire_receipts_lock(mempal_home, cwd);
    append_event_assuming_locked(mempal_home, cwd, event)
}

/// Acquire the per-project receipts flock. `None` on failure — receipts are
/// best-effort observability, so callers proceed unlocked rather than block
/// delivery. Callers holding this guard must use
/// [`append_event_assuming_locked`]; nesting [`append_event`] inside would
/// self-deadlock until the lock timeout.
pub(crate) fn acquire_receipts_lock(
    mempal_home: &Path,
    cwd: &Path,
) -> Option<crate::ingest::lock::IngestLock> {
    let path = receipts_path(mempal_home, cwd).ok()?;
    let lock_key = format!("receipts-{}", crate::ingest::lock::source_key(&path));
    crate::ingest::lock::acquire_source_lock(
        mempal_home,
        &lock_key,
        std::time::Duration::from_secs(5),
    )
    .ok()
}

pub(crate) fn append_event_assuming_locked(
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
///
/// # Errors
///
/// Returns [`InboxError`] when the receipt path cannot be encoded or an
/// existing receipt log cannot be read.
pub fn load_events(mempal_home: &Path, cwd: &Path) -> Result<Vec<ReceiptEvent>, InboxError> {
    load_events_with_completeness(mempal_home, cwd).map(|(events, _)| events)
}

pub(crate) fn load_events_with_completeness(
    mempal_home: &Path,
    cwd: &Path,
) -> Result<(Vec<ReceiptEvent>, bool), InboxError> {
    let path = receipts_path(mempal_home, cwd)?;
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), true)),
        Err(e) => return Err(e.into()),
    };

    let mut events = Vec::new();
    let mut is_complete = true;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<ReceiptEvent>(trimmed) {
            Ok(event) => events.push(event),
            Err(_) => is_complete = false,
        }
    }
    Ok((events, is_complete))
}

struct InFlightRef {
    target: &'static str,
    from: String,
    pushed_at: String,
}

fn load_in_flight(
    mempal_home: &Path,
    cwd: &Path,
) -> Result<HashMap<String, Vec<InFlightRef>>, InboxError> {
    let mut in_flight: HashMap<String, Vec<InFlightRef>> = HashMap::new();
    for target in [Tool::Claude, Tool::Codex] {
        let path = inbox::inbox_path(mempal_home, target, cwd)?;
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for line in content.lines() {
            if let Ok(message) = serde_json::from_str::<inbox::InboxMessage>(line.trim())
                && let Some(id) = message.message_id
            {
                in_flight.entry(id).or_default().push(InFlightRef {
                    target: target.dir_name(),
                    from: message.from,
                    pushed_at: message.pushed_at,
                });
            }
        }
    }
    Ok(in_flight)
}

/// Join receipt events with the live inbox files into per-message states,
/// newest `queued_at` first.
///
/// # Errors
///
/// Returns [`InboxError`] when receipt or live-inbox state cannot be read.
/// A missing inbox is valid; other inbox read errors are surfaced so they
/// cannot be misreported as `lost` delivery.
pub fn message_states(
    mempal_home: &Path,
    cwd: &Path,
) -> Result<Vec<MessageReceiptState>, InboxError> {
    let events = load_events(mempal_home, cwd)?;

    // Receipt handles still sitting in a live inbox file are pending. Keep
    // one entry per line (multiset) plus enough metadata to surface
    // messages whose `queued` receipt write failed.
    let mut in_flight = load_in_flight(mempal_home, cwd)?;

    // Multiset join: k-th queued event pairs with k-th drained event per
    // id, so duplicate ids (legacy lines, rotation remnants) never make a
    // successfully drained message look lost.
    let mut queued_by_id: HashMap<Option<String>, Vec<&ReceiptEvent>> = HashMap::new();
    let mut drained_by_id: HashMap<Option<String>, Vec<&ReceiptEvent>> = HashMap::new();
    for event in &events {
        match event.event.as_str() {
            EVENT_QUEUED => queued_by_id
                .entry(event.message_id.clone())
                .or_default()
                .push(event),
            EVENT_DRAINED => drained_by_id
                .entry(event.message_id.clone())
                .or_default()
                .push(event),
            _ => {}
        }
    }

    let mut states = Vec::new();
    let mut leftover_live: Vec<(String, InFlightRef)> = Vec::new();
    for (id, queued_events) in queued_by_id {
        let mut drained_events = drained_by_id.remove(&id).unwrap_or_default();
        let mut drained_iter = drained_events.drain(..);
        let mut live = id
            .as_ref()
            .and_then(|id| in_flight.remove(id))
            .unwrap_or_default();

        for queued in queued_events {
            let drained = drained_iter.next();
            let delivery_status = if drained.is_some() {
                "drained"
            } else if !live.is_empty() {
                live.pop();
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
                status: delivery_status.to_string(),
            });
        }

        // Drained events beyond the queued count (rotated-away queued rows).
        for event in drained_iter {
            states.push(orphan_drained_state(event));
        }
        // Live rows beyond the queued count (duplicate-id remnants) must
        // not be silently dropped — they are demonstrably pending.
        if let Some(id) = id {
            leftover_live.extend(live.into_iter().map(|entry| (id.clone(), entry)));
        }
    }

    // Drained events whose id never had a queued event (legacy lines).
    for (_, drained_events) in drained_by_id {
        for event in drained_events {
            states.push(orphan_drained_state(event));
        }
    }

    // Live inbox messages with no queued receipt at all — the best-effort
    // `queued` write failed, but the message demonstrably exists.
    let orphan_live = in_flight
        .into_iter()
        .flat_map(|(id, refs)| refs.into_iter().map(move |entry| (id.clone(), entry)));
    for (id, entry) in orphan_live.chain(leftover_live) {
        states.push(MessageReceiptState {
            message_id: Some(id),
            from: entry.from,
            to: entry.target.to_string(),
            queued_at: Some(entry.pushed_at),
            drained_at: None,
            injected_as: None,
            hook_runtime: None,
            status: "pending".to_string(),
        });
    }

    // Newest queued_at first; rows without queued_at sink to the end. The
    // remaining fields provide a stable total order for same-second events
    // after their randomized HashMap grouping.
    states.sort_by(|a, b| {
        b.queued_at
            .cmp(&a.queued_at)
            .then_with(|| a.message_id.cmp(&b.message_id))
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.drained_at.cmp(&b.drained_at))
            .then_with(|| a.status.cmp(&b.status))
            .then_with(|| a.injected_as.cmp(&b.injected_as))
            .then_with(|| a.hook_runtime.cmp(&b.hook_runtime))
    });
    Ok(states)
}

fn orphan_drained_state(event: &ReceiptEvent) -> MessageReceiptState {
    MessageReceiptState {
        message_id: event.message_id.clone(),
        from: event.from.clone(),
        to: event.to.clone(),
        queued_at: None,
        drained_at: Some(event.at.clone()),
        injected_as: event.injected_as.clone(),
        hook_runtime: event.hook_runtime.clone(),
        status: "drained".to_string(),
    }
}

/// Best-effort append one `drained` receipt event per message. Callers must
/// invoke this only after the drained messages were successfully injected;
/// keeping drain and receipt recording separate makes that ordering explicit.
pub fn record_drained(
    mempal_home: &Path,
    target: Tool,
    cwd: &Path,
    messages: &[inbox::InboxMessage],
    meta: &DrainMeta,
) {
    for message in messages {
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
    fn record_drained_appends_drained_events_with_meta() {
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
        let messages = inbox::drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        // Simulate successful injection before recording the receipt.
        record_drained(tmp_home.path(), Tool::Codex, &repo, &messages, &meta);
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
        let messages = inbox::drain(tmp_home.path(), Tool::Claude, &repo).unwrap();
        record_drained(tmp_home.path(), Tool::Claude, &repo, &messages, &meta);

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
    fn message_states_orders_equal_timestamps_by_message_id() {
        let (tmp_home, repo) = tmpdir_with_git();
        for id in [
            "msg_h", "msg_c", "msg_f", "msg_a", "msg_g", "msg_d", "msg_b", "msg_e",
        ] {
            append_event(
                tmp_home.path(),
                &repo,
                &ReceiptEvent {
                    event: EVENT_QUEUED.to_string(),
                    message_id: Some(id.to_string()),
                    from: "claude".to_string(),
                    to: "codex".to_string(),
                    at: rfc3339(0),
                    injected_as: None,
                    hook_runtime: None,
                },
            )
            .unwrap();
        }

        let states = message_states(tmp_home.path(), &repo).unwrap();
        let ids: Vec<_> = states
            .iter()
            .map(|state| state.message_id.as_deref().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "msg_a", "msg_b", "msg_c", "msg_d", "msg_e", "msg_f", "msg_g", "msg_h"
            ]
        );
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
    fn same_second_identical_pushes_get_distinct_ids_and_both_drain() {
        // Codex review P1: MCP pushed_at is second-precision, so two pushes
        // of the same content in the same second must NOT share a receipt
        // handle (the join would misreport one of them as lost).
        let (tmp_home, repo) = tmpdir_with_git();

        let first = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "same body".to_string(),
            rfc3339(0),
        )
        .unwrap();
        let second = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            "same body".to_string(),
            rfc3339(0),
        )
        .unwrap();

        assert_ne!(
            first.message_id, second.message_id,
            "same-second identical pushes must get distinct receipt handles"
        );

        let meta = DrainMeta {
            injected_as: "plain".to_string(),
            hook_runtime: None,
            drained_at: rfc3339(1),
        };
        let messages = inbox::drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        record_drained(tmp_home.path(), Tool::Codex, &repo, &messages, &meta);
        assert_eq!(messages.len(), 2);

        let states = message_states(tmp_home.path(), &repo).unwrap();
        assert_eq!(states.len(), 2, "{states:?}");
        assert!(
            states.iter().all(|s| s.status == "drained"),
            "both same-second messages must report drained: {states:?}"
        );
    }

    #[test]
    fn duplicate_id_events_join_as_multiset() {
        // Backstop for rotation/legacy paths where duplicate ids can still
        // appear in the log: k-th queued pairs with k-th drained instead of
        // a last-write-wins HashMap.
        let (tmp_home, repo) = tmpdir_with_git();
        for n in 0..2 {
            append_event(
                tmp_home.path(),
                &repo,
                &ReceiptEvent {
                    event: EVENT_QUEUED.to_string(),
                    message_id: Some("msg_dup".to_string()),
                    from: "claude".to_string(),
                    to: "codex".to_string(),
                    at: rfc3339(n),
                    injected_as: None,
                    hook_runtime: None,
                },
            )
            .unwrap();
        }
        for n in 2..4 {
            append_event(
                tmp_home.path(),
                &repo,
                &ReceiptEvent {
                    event: EVENT_DRAINED.to_string(),
                    message_id: Some("msg_dup".to_string()),
                    from: "claude".to_string(),
                    to: "codex".to_string(),
                    at: rfc3339(n),
                    injected_as: Some("plain".to_string()),
                    hook_runtime: None,
                },
            )
            .unwrap();
        }

        let states = message_states(tmp_home.path(), &repo).unwrap();
        assert_eq!(states.len(), 2, "{states:?}");
        assert!(
            states.iter().all(|s| s.status == "drained"),
            "both duplicate-id messages must report drained: {states:?}"
        );
    }

    #[test]
    fn inbox_message_without_queued_receipt_still_reported_pending() {
        // Codex review P2: a best-effort queued write can fail; a message
        // sitting in the live inbox must still surface as pending.
        let (tmp_home, repo) = tmpdir_with_git();

        let path = inbox::inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "{\"pushed_at\":\"2026-07-26T03:00:00Z\",\"from\":\"claude\",\"content\":\"receipt write failed\",\"message_id\":\"msg_orphan000001\"}\n",
        )
        .unwrap();

        let states = message_states(tmp_home.path(), &repo).unwrap();
        assert_eq!(states.len(), 1, "{states:?}");
        assert_eq!(states[0].message_id.as_deref(), Some("msg_orphan000001"));
        assert_eq!(states[0].status, "pending");
        assert_eq!(states[0].to, "codex");
        assert_eq!(states[0].queued_at.as_deref(), Some("2026-07-26T03:00:00Z"));
    }

    #[test]
    fn message_states_does_not_report_lost_when_live_inbox_is_unreadable() {
        let (tmp_home, repo) = tmpdir_with_git();
        append_event(
            tmp_home.path(),
            &repo,
            &ReceiptEvent {
                event: EVENT_QUEUED.to_string(),
                message_id: Some("msg_unreadable".to_string()),
                from: "claude".to_string(),
                to: "codex".to_string(),
                at: rfc3339(0),
                injected_as: None,
                hook_runtime: None,
            },
        )
        .unwrap();

        // A directory at the inbox file path gives a deterministic read
        // error on every supported platform without permission tricks.
        let path = inbox::inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        fs::create_dir_all(&path).unwrap();

        assert!(
            message_states(tmp_home.path(), &repo).is_err(),
            "an unreadable live inbox must not be treated as absent/lost"
        );
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_appends_never_exceed_cap() {
        // Codex review P2: unlocked load-decide-append lets two writers at
        // 399 events both append. Appends are flock-serialized on Unix.
        let (tmp_home, repo) = tmpdir_with_git();
        let home = tmp_home.path().to_path_buf();

        let handles: Vec<_> = (0..8)
            .map(|thread| {
                let home = home.clone();
                let repo = repo.clone();
                std::thread::spawn(move || {
                    for n in 0..60 {
                        append_event(
                            &home,
                            &repo,
                            &ReceiptEvent {
                                event: EVENT_QUEUED.to_string(),
                                message_id: Some(format!("msg_t{thread:02}n{n:03}")),
                                from: "claude".to_string(),
                                to: "codex".to_string(),
                                at: format!("2026-07-26T04:{thread:02}:{n:02}Z"),
                                injected_as: None,
                                hook_runtime: None,
                            },
                        )
                        .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let events = load_events(&home, &repo).unwrap();
        assert_eq!(
            events.len(),
            MAX_RECEIPT_EVENTS,
            "480 serialized appends must settle exactly at the cap"
        );
    }

    #[test]
    fn concurrent_same_input_pushes_get_unique_ids() {
        // Codex re-review P1: id selection read used-ids without holding the
        // lock, so concurrent identical pushes could pick the same handle
        // (16 concurrent pushes yielded only 3 unique ids in their repro).
        let (tmp_home, repo) = tmpdir_with_git();
        let home = tmp_home.path().to_path_buf();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let home = home.clone();
                let repo = repo.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    inbox::push_with_receipt(
                        &home,
                        Tool::Claude,
                        Tool::Codex,
                        &repo,
                        "identical burst".to_string(),
                        "2026-07-27T00:00:00Z".to_string(),
                    )
                    .map(|outcome| outcome.message_id)
                })
            })
            .collect();

        let ids: Vec<String> = handles
            .into_iter()
            .map(|h| h.join().unwrap().expect("push"))
            .collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            16,
            "all concurrent pushes must get unique handles, got {ids:?}"
        );
    }

    #[test]
    fn concurrent_same_input_pushes_stay_unique_when_receipts_lock_is_unavailable() {
        let (tmp_home, repo) = tmpdir_with_git();
        let home = tmp_home.path().to_path_buf();
        // Force `acquire_source_lock` to fail before opening its lock file.
        // Receipt observability is best-effort, but the handle returned from
        // a successful push must still be unique and trackable.
        fs::write(home.join("locks"), "not a directory").unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let home = home.clone();
                let repo = repo.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    inbox::push_with_receipt(
                        &home,
                        Tool::Claude,
                        Tool::Codex,
                        &repo,
                        "identical unlocked burst".to_string(),
                        "2026-07-27T00:00:00Z".to_string(),
                    )
                    .map(|outcome| outcome.message_id)
                })
            })
            .collect();

        let ids: Vec<String> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().expect("push"))
            .collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            16,
            "successful pushes must keep unique handles without the lock: {ids:?}"
        );
    }

    #[test]
    fn push_uses_fallback_id_when_used_id_snapshot_is_incomplete() {
        let (tmp_home, repo) = tmpdir_with_git();
        let receipt_path = receipts_path(tmp_home.path(), &repo).unwrap();
        // A directory at the log file path makes used-id collection fail,
        // while the target inbox remains writable and delivery can succeed.
        fs::create_dir_all(&receipt_path).unwrap();
        let pushed_at = "2026-07-27T00:10:00Z";
        let content = "incomplete used-id snapshot";
        let base_id = inbox::build_message_id(pushed_at, "claude", content);

        let outcome = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            content.to_string(),
            pushed_at.to_string(),
        )
        .expect("delivery must not fail with receipt IO");

        assert_ne!(
            outcome.message_id, base_id,
            "an incomplete used-id snapshot must not trust the deterministic base"
        );
        assert!(outcome.message_id.starts_with(&format!("{base_id}-u")));
    }

    #[test]
    fn push_uses_fallback_id_when_receipt_log_contains_malformed_lines() {
        let (tmp_home, repo) = tmpdir_with_git();
        let receipt_path = receipts_path(tmp_home.path(), &repo).unwrap();
        fs::create_dir_all(receipt_path.parent().unwrap()).unwrap();
        fs::write(&receipt_path, "{not valid receipt json}\n").unwrap();
        let pushed_at = "2026-07-27T00:11:00Z";
        let content = "malformed used-id snapshot";
        let base_id = inbox::build_message_id(pushed_at, "claude", content);

        let outcome = inbox::push_with_receipt(
            tmp_home.path(),
            Tool::Claude,
            Tool::Codex,
            &repo,
            content.to_string(),
            pushed_at.to_string(),
        )
        .expect("delivery must not fail with malformed receipt lines");

        assert!(
            outcome.message_id.starts_with(&format!("{base_id}-u")),
            "malformed receipt lines make the used-id snapshot incomplete"
        );
    }

    #[test]
    fn extra_live_rows_beyond_queued_still_pending() {
        // Codex re-review P2: with N live inbox lines sharing an id but only
        // M<N queued events, the leftover live rows were dropped from the
        // join instead of surfacing as pending.
        let (tmp_home, repo) = tmpdir_with_git();

        let path = inbox::inbox_path(tmp_home.path(), Tool::Codex, &repo).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            concat!(
                "{\"pushed_at\":\"2026-07-27T01:00:00Z\",\"from\":\"claude\",\"content\":\"a\",\"message_id\":\"msg_dup2\"}\n",
                "{\"pushed_at\":\"2026-07-27T01:00:01Z\",\"from\":\"claude\",\"content\":\"b\",\"message_id\":\"msg_dup2\"}\n",
            ),
        )
        .unwrap();
        append_event(
            tmp_home.path(),
            &repo,
            &ReceiptEvent {
                event: EVENT_QUEUED.to_string(),
                message_id: Some("msg_dup2".to_string()),
                from: "claude".to_string(),
                to: "codex".to_string(),
                at: "2026-07-27T01:00:00Z".to_string(),
                injected_as: None,
                hook_runtime: None,
            },
        )
        .unwrap();

        let states = message_states(tmp_home.path(), &repo).unwrap();
        assert_eq!(
            states.len(),
            2,
            "2 live rows + 1 queued must yield 2 states, got {states:?}"
        );
        assert!(
            states.iter().all(|s| s.status == "pending"),
            "all rows must be pending: {states:?}"
        );
    }

    #[test]
    fn legacy_inbox_line_without_message_id_still_records_receipt() {
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
        let messages = inbox::drain(tmp_home.path(), Tool::Codex, &repo).unwrap();
        record_drained(tmp_home.path(), Tool::Codex, &repo, &messages, &meta);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "old style");
        assert_eq!(messages[0].message_id, None);

        let events = load_events(tmp_home.path(), &repo).unwrap();
        let drained: Vec<_> = events.iter().filter(|e| e.event == EVENT_DRAINED).collect();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].message_id, None);
    }
}
