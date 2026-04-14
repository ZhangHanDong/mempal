//! Cross-agent cowork: live session peek (no storage) + decision-only ingest.
//!
//! See `docs/specs/2026-04-13-cowork-peek-and-decide.md` (P6 peek) and
//! `docs/specs/2026-04-14-p8-cowork-inbox-push.md` (P8 push).

pub mod claude;
pub mod codex;
pub mod inbox;
pub mod peek;

pub use inbox::{
    InboxError, InboxMessage, MAX_MESSAGE_SIZE, MAX_PENDING_MESSAGES, MAX_TOTAL_INBOX_BYTES,
};
pub use peek::{PeekError, PeekMessage, PeekRequest, PeekResponse, Tool, peek_partner};
