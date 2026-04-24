//! Memory protocol — behavioral instructions that teach AI agents
//! how to use mempal effectively.
//!
//! This is embedded in MCP status responses and CLI wake-up output,
//! following the same self-describing principle as `mempal-aaak::generate_spec()`:
//! the protocol lives next to the code so it cannot drift.

/// Human-readable protocol telling AI agents when and how to use mempal tools.
///
/// Returned by `mempal_status` (MCP) and displayed in `mempal wake-up` (CLI)
/// so the AI learns its own workflow from the tool response — no system prompt
/// configuration required.
pub const MEMORY_PROTOCOL: &str = r#"MEMPAL MEMORY PROTOCOL (for AI agents)

You have persistent project memory via mempal. Follow these rules in every session:

0. FIRST-TIME SETUP (once per session)
   Call mempal_status() once at the start of any session to discover available
   wings and their drawer counts. Only use wing/room filters on mempal_search
   AFTER you have seen the exact wing name in that status response (or the
   user explicitly named it). Guessing a wing (e.g. "engineering", "backend")
   silently returns zero results. When uncertain, leave wing/room unset for a
   global search.

1. WAKE UP
   Some clients (Claude Code with SessionStart hooks) pre-load recent wing/room
   context above. Others (Codex, Cursor, raw MCP clients) do NOT — for those,
   step 0 is how you wake up. Trust drawer_ids and source_file citations in
   any results you receive; they reference real files on disk.

2. VERIFY BEFORE ASSERTING
   Before stating project facts ("we chose X", "we use Y", "the auth flow is Z"),
   call mempal_search to confirm. Never guess from general knowledge when the
   user is asking about THIS project.

3. QUERY WHEN UNCERTAIN
   When the user asks about past decisions, historical context, "why did we...",
   "last time we...", or "what was the decision about...", call mempal_search
   with their question. Do not rely on conversation memory alone. You can also
   call mempal_tunnels with action="list" to discover related rooms across
   wings when context may live in another project.

3b. USE MIND-MODEL CONTEXT FOR GUIDANCE
   When you need ordered operating guidance rather than raw evidence search,
   call mempal_context. It assembles typed knowledge in the intended runtime
   order: dao_tian -> dao_ren -> shu -> qi, with evidence opt-in. Use this
   before choosing a workflow or skill when the user asks "how should we
   approach this?" or when a task benefits from high-level principles plus
   concrete tool bindings. Treat trigger_hints as metadata only — they bias
   skill choice but do not execute skills for you.

3a. TRANSLATE QUERIES TO ENGLISH
   The default embedding model is a multilingual distillation (model2vec) but
   still performs best with English queries. Non-English queries may miss
   relevant results. When the user's question is in Chinese, Japanese, Korean,
   or any other non-English language, translate the semantic intent into English
   BEFORE passing it as the query string to mempal_search. Do NOT transliterate
   — capture the meaning. Example: user says "它不再是一个高级原型" → search
   for "no longer just an advanced prototype".

4. SAVE AFTER DECISIONS
   When a decision is reached in the conversation (especially one with reasons),
   call mempal_ingest to persist it. Include the rationale, not just the
   decision. Use the current project's wing; let mempal auto-route the room.

5. CITE EVERYTHING
   Every mempal_search result includes drawer_id and source_file. Reference them
   when you answer: "according to drawer X from /path/to/file, we decided...".
   Citations are what separate memory from hallucination.

5a. KEEP A DIARY
   After completing a session's work, optionally record behavioral observations
   using mempal_ingest with wing="agent-diary" and room=your-agent-name (e.g.
   "claude", "codex"). Prefix entries with OBSERVATION:, LESSON:, or PATTERN:
   to categorize. Diary entries help future sessions of any agent learn from
   past behavioral patterns. If recording multiple entries in one day, set
   diary_rollup=true to merge them into the current UTC day's single drawer and
   reduce search noise. Example: "LESSON: always check repo docs before writing
   infrastructure code."

8. PARTNER AWARENESS (cross-agent cowork)
   When the user references the partner coding agent ("Codex 那边...",
   "ask Claude what...", "partner is working on...", "handoff..."), call
   mempal_peek_partner to read the partner's LIVE session rather than
   searching mempal drawers. Live conversation is transient and stays in
   session logs, not mempal. Use peek for CURRENT partner state; use
   mempal_search for CRYSTALLIZED past decisions. Don't conflate the two.
   Pass tool="auto" to infer the partner from the MCP client you are
   connected through, or name it explicitly (claude / codex).

9. DECISION CAPTURE (what goes into mempal)
   mempal_ingest is for decisions, not chat logs. A drawer-worthy item is
   one where the user (and you, optionally with partner agent input via
   peek) have reached a firm conclusion: an architectural choice, a
   naming/API contract, a bug root cause + patch, a spec change. Do NOT
   ingest brainstorming scratchpad, intermediate exploration, or raw
   conversation. When the decision was shaped by partner involvement
   (you called mempal_peek_partner this turn), include the partner's key
   points in the drawer body so the drawer is self-contained without
   re-peeking. Cite the partner session file path in source_file alongside
   your own citation.

10. COWORK PUSH (proactive handoff to partner)
   Call mempal_cowork_push when YOU (the agent) want the partner agent
   to see something on their next user turn. This is a SEND primitive —
   orthogonal to mempal_peek_partner (READ live state) and mempal_ingest
   (PERSIST decisions). Typical use: partner should notice a status
   update, blocker, or in-flight decision that is too transient for a
   drawer but too important for the user to have to relay manually.

   Delivery semantics: at-next-UserPromptSubmit, NOT real-time. The
   partner's TUI does not re-render on external events; delivery happens
   when the user types their next prompt in the partner's session,
   triggering the UserPromptSubmit hook which drains the inbox and
   injects via the standard hook stdout protocol.

   Addressing: pass target_tool="claude" or target_tool="codex" to
   choose explicitly, or omit to infer partner from MCP client identity.
   Self-push (target == you) is rejected.

   When NOT to push:
   - Content you also want to persist → use mempal_ingest (drawers)
   - Trigger partner mid-turn → not supported (at-next-submit only)
   - Broadcast to multiple targets → one target per push
   - Rich content / file attachments → only plain text body (≤ 8 KB)

   On InboxFull error: STOP pushing and wait for partner to drain. Do
   NOT retry — that would just fail again.

11. VERIFY BEFORE INGEST (contradiction detection)
   Before ingesting a decision that asserts relationships between named
   entities ("X is Y's Z", "X works at Y", "X is the Z of Y"), call
   mempal_fact_check with the draft text. The tool reports three kinds
   of issues:
   - SimilarNameConflict: the mentioned name is ≤2 edit-distance from a
     known entity (probable typo — Bob vs Bobby).
   - RelationContradiction: KG already records an incompatible predicate
     for the same (subject, object) endpoints.
   - StaleFact: the KG row for the asserted triple has valid_to < now.
   Treat any surfaced issue as a prompt to confirm with the user before
   persisting. Fact checking is pure read, zero LLM, zero network.
   Skip for brainstorming or scratch text — it is for load-bearing
   claims only.

TOOLS:
  mempal_status        — current state + this protocol + AAAK format spec
  mempal_search        — semantic search with wing/room filters, citation-bearing
  mempal_context       — ordered mind-model runtime context (dao_tian -> dao_ren -> shu -> qi)
  mempal_ingest        — save a new drawer (wing required, room optional, importance 0-5)
  mempal_delete        — soft-delete a drawer by ID
  mempal_taxonomy      — list or edit routing keywords
  mempal_kg            — knowledge graph: add/query/invalidate/timeline/stats triples
  mempal_tunnels       — discover cross-wing room links
  mempal_peek_partner  — read partner agent's live session (Claude ↔ Codex), pure read
  mempal_cowork_push   — send a short handoff message to partner agent (P8)
  mempal_fact_check    — offline contradiction detection vs KG triples + entities (P9)

Key invariant: mempal stores raw text verbatim. Every search result can be
traced back to a source_file. If you cannot cite the source, you are guessing."#;

/// The default identity text shown when `~/.mempal/identity.txt` does not exist.
pub const DEFAULT_IDENTITY_HINT: &str = "(identity not set — create ~/.mempal/identity.txt to define your role, projects, and working style)";

#[cfg(test)]
mod tests {
    use super::MEMORY_PROTOCOL;

    #[test]
    fn contains_rule_8_partner_awareness() {
        assert!(
            MEMORY_PROTOCOL.contains("8. PARTNER AWARENESS"),
            "MEMORY_PROTOCOL must include Rule 8 PARTNER AWARENESS"
        );
    }

    #[test]
    fn contains_rule_9_decision_capture() {
        assert!(
            MEMORY_PROTOCOL.contains("9. DECISION CAPTURE"),
            "MEMORY_PROTOCOL must include Rule 9 DECISION CAPTURE"
        );
    }

    #[test]
    fn contains_peek_partner_tool_name() {
        assert!(
            MEMORY_PROTOCOL.contains("mempal_peek_partner"),
            "MEMORY_PROTOCOL must mention the mempal_peek_partner tool"
        );
    }

    #[test]
    fn contains_rule_10_cowork_push() {
        assert!(
            MEMORY_PROTOCOL.contains("10. COWORK PUSH"),
            "MEMORY_PROTOCOL must include Rule 10 COWORK PUSH"
        );
    }

    #[test]
    fn contains_cowork_push_tool_name() {
        assert!(
            MEMORY_PROTOCOL.contains("mempal_cowork_push"),
            "MEMORY_PROTOCOL must mention mempal_cowork_push in TOOLS list"
        );
    }

    #[test]
    fn contains_context_tool_name() {
        assert!(
            MEMORY_PROTOCOL.contains("mempal_context"),
            "MEMORY_PROTOCOL must mention mempal_context in TOOLS list"
        );
    }
}
