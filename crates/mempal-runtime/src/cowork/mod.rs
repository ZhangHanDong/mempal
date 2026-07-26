//! Cross-agent cowork: live session peek (no storage) + decision-only ingest.
//!
//! See `docs/specs/2026-04-13-cowork-peek-and-decide.md` (P6 peek) and
//! `docs/specs/2026-04-14-p8-cowork-inbox-push.md` (P8 push).

pub mod bus;
pub mod claude;
pub mod codex;
pub mod inbox;
pub mod peek;
pub mod receipts;

pub use bus::{
    AgentRecord, AgentRegistry, AgentStatus, AgentStatusSummary, BusError, BusEvent,
    CoworkCaptureReport, CoworkCaptureRequest, CreateSessionRequest, DeliveryReport,
    DeliveryStatus, DoctorReport, HandoffFilters, HandoffSummary, RegisterAgentRequest,
    SendOperation, SendReport, SendRequest, TeamSession, TmuxPeek, TmuxProbeReport,
};
pub use inbox::{
    InboxError, InboxMessage, MAX_MESSAGE_SIZE, MAX_PENDING_MESSAGES, MAX_TOTAL_INBOX_BYTES,
    PushOutcome,
};
pub use peek::{PeekError, PeekMessage, PeekRequest, PeekResponse, Tool, peek_partner};
pub use receipts::{
    DrainMeta, MAX_RECEIPT_EVENTS, MessageReceiptState, ReceiptEvent, drain_with_receipt,
    message_states,
};
