//! Bidirectional cowork inbox for P8 cowork-push protocol.
//!
//! File-based ephemeral message queue between Claude Code and Codex
//! agents working in the same project (git root). Push appends a jsonl
//! entry; drain atomically renames + reads + deletes the file.
//!
//! Design: docs/specs/2026-04-14-p8-cowork-inbox-push.md
//! Spec:   specs/p8-cowork-inbox-push.spec.md

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::peek::Tool;

pub const MAX_MESSAGE_SIZE: usize = 8 * 1024;
pub const MAX_PENDING_MESSAGES: usize = 16;
pub const MAX_TOTAL_INBOX_BYTES: u64 = 32 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("message content exceeds {MAX_MESSAGE_SIZE} bytes: got {0} bytes")]
    MessageTooLarge(usize),
    #[error("invalid cwd path (contains `..` or is not absolute): {0}")]
    InvalidCwd(String),
    #[error("cannot push to self (both caller and target resolve to {0:?})")]
    SelfPush(Tool),
    #[error(
        "inbox full: {current_count} messages / {current_bytes} bytes pending \
         (limits: {MAX_PENDING_MESSAGES} messages, {MAX_TOTAL_INBOX_BYTES} bytes) — \
         partner must drain first"
    )]
    InboxFull {
        current_count: usize,
        current_bytes: u64,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub pushed_at: String,
    pub from: String,
    pub content: String,
}

// Implementations added in subsequent tasks:
// - Task 2: mempal_home / project_identity / encode_project_identity / inbox_path
// - Task 3: push
// - Task 4: drain
// - Task 5: format_plain / format_codex_hook_json
