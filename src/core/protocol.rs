#![warn(clippy::all)]

pub use mempal_mcp_protocol::{DEFAULT_IDENTITY_HINT, MEMORY_PROTOCOL};

#[cfg(test)]
mod tests {
    use super::MEMORY_PROTOCOL;

    #[test]
    fn test_protocol_mentions_partner_awareness_and_decision_capture() {
        assert!(
            MEMORY_PROTOCOL.contains("8. PARTNER AWARENESS"),
            "MEMORY_PROTOCOL must include Rule 8 PARTNER AWARENESS"
        );
        assert!(
            MEMORY_PROTOCOL.contains("9. DECISION CAPTURE"),
            "MEMORY_PROTOCOL must include Rule 9 DECISION CAPTURE"
        );
        assert!(
            MEMORY_PROTOCOL.contains("mempal_peek_partner"),
            "MEMORY_PROTOCOL must mention the mempal_peek_partner tool"
        );
        assert!(
            MEMORY_PROTOCOL.contains("mempal_session_peek"),
            "MEMORY_PROTOCOL must mention the mempal_session_peek tool"
        );
        assert!(
            MEMORY_PROTOCOL.contains("same-tool") && MEMORY_PROTOCOL.contains("cwd"),
            "MEMORY_PROTOCOL must distinguish explicit same-tool session reads"
        );
    }
}
