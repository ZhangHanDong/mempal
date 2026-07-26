use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
#[cfg(feature = "rest")]
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mempal::aaak::{AaakCodec, AaakMeta};
use mempal::adoption_analytics::{
    RuntimeAdoptionAnalyticsReport, build_runtime_adoption_analytics,
};
#[cfg(feature = "rest")]
use mempal::api::{ApiState, DEFAULT_REST_ADDR, serve as serve_rest_api};
use mempal::brief::{BriefRequest, CognitiveBrief, assemble_brief};
use mempal::context::{ContextPack, ContextRequest, assemble_context};
use mempal::core::{
    anchor,
    config::Config,
    db::{CURRENT_SCHEMA_VERSION, Database},
    phase3::{
        CardContextDefaultProposalReport, CardContextRollbackControlReport, EvaluatorAdviceInput,
        EvaluatorAdviceReport, Phase3ReadinessReport, ResearchIngestPlanReport,
        RuntimeAdoptionCaptureInput, RuntimeAdoptionCaptureReport,
        RuntimeAdoptionCheckedRecordReport, RuntimeAdoptionGuidance, RuntimeAdoptionRecordPlan,
        RuntimeAdoptionRecordPlanInput, RuntimeAdoptionRecordQualityReport,
        RuntimeAdoptionReviewFilters, RuntimeAdoptionReviewReport,
        build_research_ingest_plan_from_value, capture_runtime_adoption_record_input,
        card_context_default_proposal, card_context_default_readiness,
        card_context_rollback_control, check_runtime_adoption_record, evaluator_advice,
        prepare_runtime_adoption_capture, prepare_runtime_adoption_record,
        review_runtime_adoption_events, runtime_adoption_guidance,
        runtime_adoption_instrumentation_policy, should_write_checked_record,
    },
    protocol::{DEFAULT_IDENTITY_HINT, MEMORY_PROTOCOL},
    types::{
        AnchorKind, Drawer, KnowledgeCard, KnowledgeCardEvent, KnowledgeCardFilter,
        KnowledgeEventType, KnowledgeEvidenceLink, KnowledgeEvidenceRole, KnowledgeStatus,
        KnowledgeTier, MemoryDomain, MemoryKind, Provenance, RuntimeAdoptionEvent,
        RuntimeAdoptionFilter, RuntimeAdoptionSignal, RuntimeAdoptionTrack, SourceType,
        TaxonomyEntry, TriggerHints, TunnelEndpoint,
    },
    utils::{build_triple_id, current_timestamp, format_tunnel_endpoint},
};
use mempal::doctor::{DoctorReport, build_doctor_report};
use mempal::embed::{ConfiguredEmbedderFactory, Embedder};
use mempal::field_taxonomy::{FieldTaxonomyEntry, field_taxonomy};
use mempal::ingest::{
    IngestOptions, IngestStats, ingest_dir_with_options, ingest_file_with_options,
    normalize::CURRENT_NORMALIZE_VERSION,
    reindex::{ReindexMode, ReindexOptions, ReindexReport, reindex_sources},
};
use mempal::knowledge_anchor::{PublishAnchorRequest, publish_anchor};
use mempal::knowledge_card_backfill::{
    KnowledgeCardBackfillApplyOptions, KnowledgeCardBackfillApplyResult,
    KnowledgeCardBackfillReport, apply_backfill, build_backfill_report,
};
use mempal::knowledge_card_lifecycle::{
    DemoteCardOutcome, DemoteCardRequest, KnowledgeCardGateReport, PromoteCardOutcome,
    PromoteCardRequest, demote_card, evaluate_card_gate_by_id, promote_card,
};
use mempal::knowledge_card_retrieval::{
    KnowledgeCardRetrievalRequest, RetrievedKnowledgeCard, retrieve_knowledge_cards,
};
use mempal::knowledge_distill::{DistillPlan, DistillRequest, commit_distill, prepare_distill};
use mempal::knowledge_gate::{
    GateReport, PromotionPolicyEntry, evaluate_gate_by_id, promotion_policy,
};
use mempal::knowledge_lifecycle::{
    DemoteRequest, PromoteRequest, demote_knowledge, promote_knowledge,
};
use mempal::mcp::MempalMcpServer;
use mempal::path_filter::{ProjectPathFilterOptions, project_walk};
use mempal::search::{SearchFilters, SearchOptions, search_with_options};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

mod longmemeval;

use crate::longmemeval::{BenchMode, LongMemEvalArgs, LongMemEvalGranularity, default_top_k};

#[derive(Parser)]
#[command(name = "mempal", about = "Project memory for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Doctor {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Init {
        dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "ignore-file")]
        ignore_files: Vec<PathBuf>,
        #[arg(long)]
        no_gitignore: bool,
        #[arg(long)]
        no_mempalignore: bool,
    },
    Ingest {
        dir: PathBuf,
        #[arg(long)]
        wing: String,
        #[arg(long)]
        room: Option<String>,
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        no_strip_noise: bool,
        #[arg(long)]
        diary_rollup: bool,
        #[arg(long = "ignore-file")]
        ignore_files: Vec<PathBuf>,
        #[arg(long)]
        no_gitignore: bool,
        #[arg(long)]
        no_mempalignore: bool,
    },
    Search {
        query: String,
        #[arg(long)]
        wing: Option<String>,
        #[arg(long)]
        room: Option<String>,
        #[arg(long)]
        memory_kind: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        field: Option<String>,
        #[arg(long)]
        tier: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        anchor_kind: Option<String>,
        #[arg(long, default_value_t = 10)]
        top_k: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        with_neighbors: bool,
    },
    Context {
        query: String,
        #[arg(long, default_value = "general")]
        field: String,
        #[arg(long, default_value = "project")]
        domain: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long, default_value = "plain")]
        format: String,
        #[arg(long)]
        include_evidence: bool,
        #[arg(long)]
        include_cards: bool,
        #[arg(long, conflicts_with = "include_cards")]
        no_include_cards: bool,
        #[arg(long, default_value_t = 12)]
        max_items: usize,
        #[arg(long = "dao-tian-limit", default_value_t = 1)]
        dao_tian_limit: usize,
        #[arg(long = "no-distill-suggestions")]
        no_distill_suggestions: bool,
    },
    Brief {
        query: String,
        #[arg(long, default_value = "general")]
        field: String,
        #[arg(long, default_value = "project")]
        domain: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long, default_value = "plain")]
        format: String,
        #[arg(long, default_value_t = 12)]
        max_items: usize,
        #[arg(long = "dao-tian-limit", default_value_t = 1)]
        dao_tian_limit: usize,
    },
    /// List every project (wing) mempal knows, newest activity first.
    Projects {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Resume a project by fuzzy name from any directory.
    Resume {
        /// Project name or fragment (matches a wing or a worktree-path basename).
        query: String,
        #[arg(long, default_value_t = 5)]
        evidence_limit: usize,
        #[arg(long, default_value_t = 5)]
        candidate_limit: usize,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    WakeUp {
        #[arg(long)]
        format: Option<String>,
    },
    Compress {
        text: String,
    },
    Bench {
        #[command(subcommand)]
        command: BenchCommands,
    },
    Delete {
        drawer_id: String,
    },
    Purge {
        /// Only purge drawers soft-deleted before this ISO timestamp
        #[arg(long)]
        before: Option<String>,
    },
    Reindex {
        #[arg(long)]
        stale: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Kg {
        #[command(subcommand)]
        command: KgCommands,
    },
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
    },
    KnowledgeCard {
        #[command(subcommand)]
        command: KnowledgeCardCommands,
    },
    Phase3 {
        #[command(subcommand)]
        command: Phase3Commands,
    },
    Tunnels {
        #[command(subcommand)]
        command: Option<TunnelCommands>,
    },
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    FieldTaxonomy {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Serve {
        #[arg(long)]
        mcp: bool,
    },
    Status,
    /// Run offline contradiction check on text against KG triples +
    /// known-entity registry. Pure read, no LLM, no network.
    FactCheck {
        /// File path or `-` for stdin. Omit for stdin.
        path: Option<PathBuf>,
        /// Optional wing filter for known-entity scope.
        #[arg(long)]
        wing: Option<String>,
        /// Optional room filter within the wing.
        #[arg(long)]
        room: Option<String>,
        /// RFC3339 timestamp for the `now` cutoff (stale-fact detection).
        /// Defaults to the current UTC time.
        #[arg(long)]
        now: Option<String>,
    },
    /// Drain cowork inbox messages for the given target. Always exits 0
    /// (hook graceful degrade). Intended to be called from a UserPromptSubmit
    /// hook on each user turn — never blocks the user's prompt.
    CoworkDrain {
        /// Which agent's inbox to drain ("claude" or "codex"). Use "$MY_TOOL".
        #[arg(long)]
        target: String,

        /// Project cwd. Exactly ONE of --cwd or --cwd-source must be set.
        /// Use this for Claude Code hook (pass ${CLAUDE_PROJECT_CWD:-$PWD}).
        #[arg(long, conflicts_with = "cwd_source")]
        cwd: Option<PathBuf>,

        /// Alternative cwd source for hooks whose runtime provides a
        /// structured input payload. Currently supported: "stdin-json"
        /// (reads stdin as JSON and extracts the `cwd` field, per Codex's
        /// UserPromptSubmitCommandInput schema).
        #[arg(long, conflicts_with = "cwd")]
        cwd_source: Option<String>,

        /// Output format: "plain" for Claude Code hook (prepend to prompt),
        /// or "codex-hook-json" for Codex native hook envelope.
        #[arg(long, default_value = "plain")]
        format: String,

        /// Optional hook runtime label recorded in delivery receipts
        /// (e.g. "claude UserPromptSubmit"). P116.
        #[arg(long)]
        hook_runtime: Option<String>,
    },
    /// Show current cowork inbox state for both targets at the given cwd
    /// (read-only — does NOT drain).
    CoworkStatus {
        #[arg(long)]
        cwd: PathBuf,
    },
    /// Show pair-push delivery receipts for the given cwd: per-message
    /// pending / drained / lost states derived from the receipts log (P116,
    /// read-only).
    CoworkReceipts {
        #[arg(long)]
        cwd: PathBuf,

        /// Maximum number of messages to show (newest first).
        #[arg(long)]
        limit: Option<usize>,

        /// Output format: "plain" or "json".
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Print the multi-agent cowork runbook.
    CoworkRunbook {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Install cowork hooks: Claude Code (project-level .claude/hooks)
    /// and optionally Codex (global ~/.codex/hooks.json merge).
    CoworkInstallHooks {
        #[arg(long, default_value_t = false)]
        global_codex: bool,
    },
    /// Register or update a concrete agent instance in the multi-agent cowork
    /// bus.
    CoworkRegister {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        tool: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value = "inbox")]
        transport: String,
        #[arg(long)]
        tmux_target: Option<String>,
    },
    /// Send one ephemeral message to one concrete agent id.
    CoworkSend {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long)]
        thread_id: Option<String>,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Fan out one ephemeral message to several concrete agent ids.
    CoworkBroadcast {
        #[arg(long)]
        from: String,
        #[arg(long = "to", required = true)]
        to: Vec<String>,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long)]
        thread_id: Option<String>,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Drain one concrete agent id's multi-agent bus inbox.
    CoworkAgentDrain {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// List registered concrete agents and their per-agent inbox state.
    CoworkAgents {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        now: Option<String>,
    },
    /// Replay append-only multi-agent cowork bus events.
    CoworkEvents {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Acknowledge one delivery message id for a concrete agent.
    CoworkAck {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        message_id: String,
    },
    /// Replay delivery status derived from the multi-agent cowork event log.
    CoworkDeliveries {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Update one concrete agent's explicit heartbeat timestamp.
    CoworkHeartbeat {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        seen_at: Option<String>,
    },
    /// Replace one channel membership list with concrete agent ids.
    CoworkChannelSet {
        #[arg(long)]
        channel: String,
        #[arg(long = "agent", required = true)]
        agent: Vec<String>,
        #[arg(long)]
        cwd: PathBuf,
    },
    /// Send one message to all current members of a channel.
    CoworkChannelSend {
        #[arg(long)]
        from: String,
        #[arg(long)]
        channel: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long)]
        thread_id: Option<String>,
    },
    /// Read-only capture of a registered tmux agent pane.
    CoworkTmuxPeek {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value_t = 80)]
        lines: usize,
    },
    /// Read a partner agent's LIVE session log for a project (no tmux needed).
    CoworkPeek {
        /// Which agent tool's session to read (claude|codex).
        #[arg(long)]
        tool: String,
        /// Project directory whose partner session to read.
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Diagnose multi-agent cowork bus runtime state.
    CoworkDoctor {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        now: Option<String>,
        #[arg(long)]
        probe_tmux: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Create or replace a runtime team session.
    CoworkSessionCreate {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long = "agent", required = true)]
        agent: Vec<String>,
        #[arg(long = "channel")]
        channel: Vec<String>,
        #[arg(long)]
        thread_id: Option<String>,
    },
    /// List runtime team sessions.
    CoworkSessions {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Update a runtime team session status.
    CoworkSessionStatus {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        status: String,
    },
    /// Close a runtime team session and optionally capture its handoff summary.
    CoworkSessionClose {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        capture: bool,
        #[arg(long)]
        execute: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Build a deterministic multi-agent handoff summary.
    CoworkHandoff {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        thread_id: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Explicitly capture a cowork handoff summary into evidence memory.
    CoworkCapture {
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value = "handoff")]
        summary_source: String,
        #[arg(long, default_value = "cowork-capture")]
        wing: String,
        #[arg(long)]
        room: Option<String>,
        #[arg(long)]
        thread_id: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        execute: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Print the governed maintenance runbook.
    MaintenanceRunbook {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Maintenance {
        #[command(subcommand)]
        command: MaintenanceCommands,
    },
    ReleaseReadiness {
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
enum MaintenanceCommands {
    GuidedRun {
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
enum TaxonomyCommands {
    List,
    Edit {
        wing: String,
        room: String,
        #[arg(long)]
        keywords: String,
    },
}

#[derive(Subcommand)]
enum KgCommands {
    Add {
        subject: String,
        predicate: String,
        object: String,
        #[arg(long)]
        source_drawer: Option<String>,
    },
    Query {
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        predicate: Option<String>,
        #[arg(long)]
        object: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Timeline {
        entity: String,
    },
    Stats,
    List,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum KnowledgeCommands {
    Distill {
        #[arg(long)]
        statement: String,
        #[arg(long)]
        content: String,
        #[arg(long)]
        tier: String,
        #[arg(long = "supporting-ref", required = true)]
        supporting_refs: Vec<String>,
        #[arg(long, default_value = "mempal")]
        wing: String,
        #[arg(long, default_value = "knowledge")]
        room: String,
        #[arg(long, default_value = "project")]
        domain: String,
        #[arg(long, default_value = "general")]
        field: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long = "scope-constraints")]
        scope_constraints: Option<String>,
        #[arg(long = "counterexample-ref")]
        counterexample_refs: Vec<String>,
        #[arg(long = "teaching-ref")]
        teaching_refs: Vec<String>,
        #[arg(long = "intent-tag")]
        intent_tags: Vec<String>,
        #[arg(long = "workflow-bias")]
        workflow_bias: Vec<String>,
        #[arg(long = "tool-need")]
        tool_needs: Vec<String>,
        #[arg(long, default_value_t = 2)]
        importance: i32,
        #[arg(long)]
        dry_run: bool,
    },
    Promote {
        drawer_id: String,
        #[arg(long)]
        status: String,
        #[arg(long = "verification-ref", required = true)]
        verification_refs: Vec<String>,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        reviewer: Option<String>,
    },
    Demote {
        drawer_id: String,
        #[arg(long)]
        status: String,
        #[arg(long = "evidence-ref", required = true)]
        evidence_refs: Vec<String>,
        #[arg(long)]
        reason: String,
        #[arg(long = "reason-type")]
        reason_type: String,
    },
    Gate {
        drawer_id: String,
        #[arg(long = "target-status")]
        target_status: Option<String>,
        #[arg(long)]
        reviewer: Option<String>,
        #[arg(long = "allow-counterexamples")]
        allow_counterexamples: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Policy {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    PublishAnchor {
        drawer_id: String,
        #[arg(long)]
        to: String,
        #[arg(long = "target-anchor-id")]
        target_anchor_id: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        reviewer: Option<String>,
    },
}

#[derive(Subcommand)]
enum KnowledgeCardCommands {
    Create {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        statement: String,
        #[arg(long)]
        content: String,
        #[arg(long)]
        tier: String,
        #[arg(long)]
        status: String,
        #[arg(long, default_value = "project")]
        domain: String,
        #[arg(long, default_value = "general")]
        field: String,
        #[arg(long = "anchor-kind", default_value = "repo")]
        anchor_kind: String,
        #[arg(long = "anchor-id")]
        anchor_id: String,
        #[arg(long = "parent-anchor-id")]
        parent_anchor_id: Option<String>,
        #[arg(long = "scope-constraints")]
        scope_constraints: Option<String>,
        #[arg(long = "intent-tag")]
        intent_tags: Vec<String>,
        #[arg(long = "workflow-bias")]
        workflow_bias: Vec<String>,
        #[arg(long = "tool-need")]
        tool_needs: Vec<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Get {
        card_id: String,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    List {
        #[arg(long)]
        tier: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        field: Option<String>,
        #[arg(long = "anchor-kind")]
        anchor_kind: Option<String>,
        #[arg(long = "anchor-id")]
        anchor_id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Retrieve {
        query: String,
        #[arg(long, default_value = "project")]
        domain: String,
        #[arg(long, default_value = "general")]
        field: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long = "top-k", default_value_t = 5)]
        top_k: usize,
        #[arg(long = "evidence-top-k", default_value_t = 20)]
        evidence_top_k: usize,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Link {
        card_id: String,
        evidence_drawer_id: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        id: Option<String>,
    },
    Event {
        card_id: String,
        #[arg(long = "type")]
        event_type: String,
        #[arg(long)]
        reason: String,
        #[arg(long = "from-status")]
        from_status: Option<String>,
        #[arg(long = "to-status")]
        to_status: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
    },
    Events {
        card_id: String,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Gate {
        card_id: String,
        #[arg(long = "target-status")]
        target_status: Option<String>,
        #[arg(long)]
        reviewer: Option<String>,
        #[arg(long, default_value_t = false)]
        allow_counterexamples: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Promote {
        card_id: String,
        #[arg(long)]
        status: String,
        #[arg(long = "verification-ref")]
        verification_refs: Vec<String>,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        reviewer: Option<String>,
        #[arg(long, default_value_t = false)]
        allow_counterexamples: bool,
        #[arg(long, default_value_t = true)]
        enforce_gate: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Demote {
        card_id: String,
        #[arg(long)]
        status: String,
        #[arg(long = "evidence-ref")]
        evidence_refs: Vec<String>,
        #[arg(long)]
        reason: String,
        #[arg(long = "reason-type")]
        reason_type: String,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    BackfillPlan {
        #[arg(long)]
        tier: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        field: Option<String>,
        #[arg(long = "anchor-kind")]
        anchor_kind: Option<String>,
        #[arg(long = "anchor-id")]
        anchor_id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    BackfillApply {
        #[arg(long)]
        tier: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        field: Option<String>,
        #[arg(long = "anchor-kind")]
        anchor_kind: Option<String>,
        #[arg(long = "anchor-id")]
        anchor_id: Option<String>,
        #[arg(long)]
        execute: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // clap command enums favor direct argument fields over boxing.
enum Phase3Commands {
    Adoption {
        #[command(subcommand)]
        command: Phase3AdoptionCommands,
    },
    Evaluator {
        #[command(subcommand)]
        command: Phase3EvaluatorCommands,
    },
    DefaultProposal {
        candidate: String,
        #[arg(long = "rollback-criterion")]
        rollback_criteria: Vec<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    DefaultControl {
        candidate: String,
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        #[arg(long = "rollback-criterion")]
        rollback_criteria: Vec<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    RollbackControl {
        candidate: String,
        #[arg(long)]
        execute: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Gate {
        candidate: String,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Readiness {
        candidate: String,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    ResearchValidatePlan {
        path: PathBuf,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    ResearchIngestPlan {
        path: PathBuf,
        #[arg(long)]
        execute: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
enum Phase3EvaluatorCommands {
    Advise {
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "subject-kind")]
        subject_kind: String,
        #[arg(long = "subject-id")]
        subject_id: String,
        #[arg(long = "proposed-action")]
        proposed_action: String,
        #[arg(long = "evidence-ref")]
        evidence_refs: Vec<String>,
        #[arg(long = "counterexample-ref")]
        counterexample_refs: Vec<String>,
        #[arg(long = "risk-note")]
        risk_notes: Vec<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // `record` intentionally carries the full event payload.
enum Phase3AdoptionCommands {
    Guidance {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    InstrumentationPolicy {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    PrepareRecord {
        #[arg(long)]
        track: String,
        #[arg(long)]
        signal: String,
        #[arg(long)]
        feature: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Capture {
        #[arg(long)]
        surface: String,
        #[arg(long)]
        outcome: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        execute: bool,
        #[arg(long = "allow-warnings")]
        allow_warnings: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Wrap {
        #[arg(long)]
        surface: String,
        #[arg(long, default_value = "auto")]
        outcome: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        execute: bool,
        #[arg(long = "allow-warnings")]
        allow_warnings: bool,
        #[arg(long, default_value = "plain")]
        format: String,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    CheckRecord {
        #[arg(long)]
        track: String,
        #[arg(long)]
        signal: String,
        #[arg(long)]
        feature: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    RecordChecked {
        #[arg(long)]
        track: String,
        #[arg(long)]
        signal: String,
        #[arg(long)]
        feature: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long = "allow-warnings")]
        allow_warnings: bool,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Review {
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long)]
        signal: Option<String>,
        #[arg(long, default_value_t = 10_000)]
        limit: usize,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Analytics {
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Record {
        #[arg(long)]
        track: String,
        #[arg(long)]
        signal: String,
        #[arg(long)]
        feature: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "context-hash")]
        context_hash: Option<String>,
        #[arg(long = "card-id")]
        card_id: Option<String>,
        #[arg(long = "evaluator-id")]
        evaluator_id: Option<String>,
        #[arg(long = "research-report-id")]
        research_report_id: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "metadata-json")]
        metadata_json: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    List {
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value = "plain")]
        format: String,
    },
    Stats {
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        feature: Option<String>,
        #[arg(long, default_value = "plain")]
        format: String,
    },
}

#[derive(Subcommand)]
enum TunnelCommands {
    Add {
        #[arg(long)]
        left: String,
        #[arg(long)]
        right: String,
        #[arg(long)]
        label: String,
    },
    List {
        #[arg(long)]
        wing: Option<String>,
        #[arg(long, default_value = "all")]
        kind: String,
    },
    Delete {
        tunnel_id: String,
    },
    Follow {
        #[arg(long)]
        from: String,
        #[arg(long, default_value_t = 1)]
        hops: u8,
    },
}

#[derive(Subcommand)]
enum BenchCommands {
    #[command(name = "longmemeval")]
    LongMemEval {
        data_file: PathBuf,
        #[arg(long, value_enum, default_value_t = BenchMode::Raw)]
        mode: BenchMode,
        #[arg(long, value_enum, default_value_t = LongMemEvalGranularity::Session)]
        granularity: LongMemEvalGranularity,
        #[arg(long, default_value_t = 0)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        skip: usize,
        #[arg(long, default_value_t = default_top_k())]
        top_k: usize,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        for cause in error.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Cowork commands must graceful-degrade without requiring palace.db
    // or config to exist. Dispatch them BEFORE Config::load / Database::open
    // so a missing mempal_home never breaks the hook path.
    match cli.command {
        Commands::Doctor { format } => {
            return doctor_command(&format);
        }
        Commands::ReleaseReadiness { format } => {
            return release_readiness_command(&format);
        }
        Commands::Maintenance {
            command: MaintenanceCommands::GuidedRun { format },
        } => {
            return maintenance_guided_run_command(&format);
        }
        Commands::CoworkDrain {
            target,
            cwd,
            cwd_source,
            format,
            hook_runtime,
        } => {
            return cowork_drain_command(target, cwd, cwd_source, format, hook_runtime);
        }
        Commands::CoworkStatus { cwd } => {
            return cowork_status_command(cwd);
        }
        Commands::CoworkReceipts { cwd, limit, format } => {
            return cowork_receipts_command(cwd, limit, format);
        }
        Commands::CoworkRunbook { format } => {
            return static_runbook_command(
                "Multi-Agent Cowork Runbook",
                include_str!("../docs/COWORK-RUNBOOK.md"),
                &format,
            );
        }
        Commands::CoworkInstallHooks { global_codex } => {
            return cowork_install_hooks_command(global_codex);
        }
        Commands::CoworkRegister {
            agent_id,
            tool,
            cwd,
            transport,
            tmux_target,
        } => {
            return cowork_register_command(agent_id, tool, cwd, transport, tmux_target);
        }
        Commands::CoworkSend {
            from,
            to,
            cwd,
            message,
            thread_id,
            channel,
        } => {
            return cowork_send_command(
                from,
                vec![to],
                cwd,
                message,
                mempal::cowork::bus::SendOperation::Send,
                thread_id,
                channel,
            );
        }
        Commands::CoworkBroadcast {
            from,
            to,
            cwd,
            message,
            thread_id,
            channel,
        } => {
            return cowork_send_command(
                from,
                to,
                cwd,
                message,
                mempal::cowork::bus::SendOperation::Broadcast,
                thread_id,
                channel,
            );
        }
        Commands::CoworkAgentDrain {
            agent_id,
            cwd,
            format,
        } => {
            return cowork_agent_drain_command(agent_id, cwd, format);
        }
        Commands::CoworkAgents { cwd, now } => {
            return cowork_agents_command(cwd, now);
        }
        Commands::CoworkEvents { cwd, limit, format } => {
            return cowork_events_command(cwd, limit, format);
        }
        Commands::CoworkAck {
            agent_id,
            cwd,
            message_id,
        } => {
            return cowork_ack_command(agent_id, cwd, message_id);
        }
        Commands::CoworkDeliveries {
            cwd,
            agent_id,
            format,
        } => {
            return cowork_deliveries_command(cwd, agent_id, format);
        }
        Commands::CoworkHeartbeat {
            agent_id,
            cwd,
            seen_at,
        } => {
            return cowork_heartbeat_command(agent_id, cwd, seen_at);
        }
        Commands::CoworkChannelSet {
            channel,
            agent,
            cwd,
        } => {
            return cowork_channel_set_command(channel, agent, cwd);
        }
        Commands::CoworkChannelSend {
            from,
            channel,
            cwd,
            message,
            thread_id,
        } => {
            return cowork_channel_send_command(from, channel, cwd, message, thread_id);
        }
        Commands::CoworkTmuxPeek {
            agent_id,
            cwd,
            lines,
        } => {
            return cowork_tmux_peek_command(agent_id, cwd, lines);
        }
        Commands::CoworkPeek {
            tool,
            cwd,
            limit,
            since,
            format,
        } => {
            return cowork_peek_command(tool, cwd, limit, since, format);
        }
        Commands::CoworkDoctor {
            cwd,
            now,
            probe_tmux,
            format,
        } => {
            return cowork_doctor_command(cwd, now, probe_tmux, format);
        }
        Commands::CoworkSessionCreate {
            cwd,
            session_id,
            title,
            goal,
            agent,
            channel,
            thread_id,
        } => {
            return cowork_session_create_command(
                cwd, session_id, title, goal, agent, channel, thread_id,
            );
        }
        Commands::CoworkSessions { cwd, format } => {
            return cowork_sessions_command(cwd, format);
        }
        Commands::CoworkSessionStatus {
            cwd,
            session_id,
            status,
        } => {
            return cowork_session_status_command(cwd, session_id, status);
        }
        Commands::CoworkSessionClose {
            cwd,
            session_id,
            capture,
            execute,
            format,
        } => {
            return cowork_session_close_command(cwd, session_id, capture, execute, format);
        }
        Commands::CoworkHandoff {
            cwd,
            thread_id,
            channel,
            session_id,
            limit,
            format,
        } => {
            return cowork_handoff_command(cwd, thread_id, channel, session_id, limit, format);
        }
        Commands::CoworkCapture {
            cwd,
            summary_source,
            wing,
            room,
            thread_id,
            channel,
            session_id,
            note,
            execute,
            format,
        } => {
            return cowork_capture_command(CoworkCaptureCommandArgs {
                cwd,
                summary_source,
                wing,
                room,
                thread_id,
                channel,
                session_id,
                note,
                execute,
                format,
            });
        }
        Commands::MaintenanceRunbook { format } => {
            return static_runbook_command(
                "Maintenance Runbook",
                include_str!("../docs/MAINTENANCE-RUNBOOK.md"),
                &format,
            );
        }
        // All other commands fall through to the db-backed dispatch below.
        _ => {}
    }

    let config = Config::load().context("failed to load config")?;
    let db = Database::open(&expand_home(&config.db_path)).context("failed to open database")?;

    match cli.command {
        Commands::Init {
            dir,
            dry_run,
            ignore_files,
            no_gitignore,
            no_mempalignore,
        } => init_command(
            &db,
            &dir,
            dry_run,
            ProjectPathFilterOptions {
                respect_gitignore: !no_gitignore,
                respect_mempalignore: !no_mempalignore,
                custom_ignore_files: ignore_files,
            },
        ),
        Commands::Ingest {
            dir,
            wing,
            room,
            format,
            dry_run,
            no_strip_noise,
            diary_rollup,
            ignore_files,
            no_gitignore,
            no_mempalignore,
        } => {
            ingest_command(
                &db,
                &config,
                IngestCommandArgs {
                    dir: &dir,
                    wing: &wing,
                    room: room.as_deref(),
                    format,
                    dry_run,
                    no_strip_noise,
                    diary_rollup,
                    project_filter: ProjectPathFilterOptions {
                        respect_gitignore: !no_gitignore,
                        respect_mempalignore: !no_mempalignore,
                        custom_ignore_files: ignore_files,
                    },
                },
            )
            .await
        }
        Commands::Search {
            query,
            wing,
            room,
            memory_kind,
            domain,
            field,
            tier,
            status,
            anchor_kind,
            top_k,
            json,
            with_neighbors,
        } => {
            search_command(
                &db,
                &config,
                SearchCommandArgs {
                    query: &query,
                    wing: wing.as_deref(),
                    room: room.as_deref(),
                    filters: SearchFilters {
                        memory_kind,
                        domain,
                        field,
                        tier,
                        status,
                        anchor_kind,
                    },
                    top_k,
                    json,
                    with_neighbors,
                },
            )
            .await
        }
        Commands::Context {
            query,
            field,
            domain,
            cwd,
            format,
            include_evidence,
            include_cards,
            no_include_cards,
            max_items,
            dao_tian_limit,
            no_distill_suggestions,
        } => {
            context_command(
                &db,
                &config,
                ContextCommandArgs {
                    query,
                    field,
                    domain,
                    cwd,
                    format,
                    include_evidence,
                    include_cards,
                    no_include_cards,
                    max_items,
                    dao_tian_limit,
                    no_distill_suggestions,
                },
            )
            .await
        }
        Commands::Brief {
            query,
            field,
            domain,
            cwd,
            format,
            max_items,
            dao_tian_limit,
        } => {
            brief_command(
                &db,
                &config,
                BriefCommandArgs {
                    query,
                    field,
                    domain,
                    cwd,
                    format,
                    max_items,
                    dao_tian_limit,
                },
            )
            .await
        }
        Commands::Projects { format } => projects_command(&db, &format),
        Commands::Resume {
            query,
            evidence_limit,
            candidate_limit,
            format,
        } => resume_command(&db, &query, evidence_limit, candidate_limit, &format),
        Commands::Delete { drawer_id } => delete_command(&db, &drawer_id),
        Commands::Purge { before } => purge_command(&db, before.as_deref()),
        Commands::WakeUp { format } => wake_up_command(&db, format.as_deref()),
        Commands::Compress { text } => compress_command(&text),
        Commands::Bench { command } => bench_command(&config, command).await,
        Commands::Reindex {
            stale,
            force,
            dry_run,
        } => reindex_command(&db, &config, stale, force, dry_run).await,
        Commands::Kg { command } => kg_command(&db, command),
        Commands::Knowledge { command } => knowledge_command(&db, &config, command).await,
        Commands::KnowledgeCard { command } => knowledge_card_command(&db, &config, command).await,
        Commands::Phase3 { command } => phase3_command(&db, &config, command).await,
        Commands::Tunnels { command } => tunnels_command(&db, command),
        Commands::Taxonomy { command } => taxonomy_command(&db, command),
        Commands::FieldTaxonomy { format } => field_taxonomy_command(&format),
        Commands::Serve { mcp } => serve_command(&config, mcp).await,
        Commands::Status => status_command(&db),
        Commands::FactCheck {
            path,
            wing,
            room,
            now,
        } => fact_check_command(&db, path.as_deref(), wing.as_deref(), room.as_deref(), now),
        // Cowork commands were already dispatched above and returned early.
        Commands::CoworkDrain { .. }
        | Commands::CoworkStatus { .. }
        | Commands::CoworkReceipts { .. }
        | Commands::CoworkRunbook { .. }
        | Commands::CoworkInstallHooks { .. }
        | Commands::CoworkRegister { .. }
        | Commands::CoworkSend { .. }
        | Commands::CoworkBroadcast { .. }
        | Commands::CoworkAgentDrain { .. }
        | Commands::CoworkAgents { .. }
        | Commands::CoworkEvents { .. }
        | Commands::CoworkAck { .. }
        | Commands::CoworkDeliveries { .. }
        | Commands::CoworkHeartbeat { .. }
        | Commands::CoworkChannelSet { .. }
        | Commands::CoworkChannelSend { .. }
        | Commands::CoworkTmuxPeek { .. }
        | Commands::CoworkPeek { .. }
        | Commands::CoworkDoctor { .. }
        | Commands::CoworkSessionCreate { .. }
        | Commands::CoworkSessions { .. }
        | Commands::CoworkSessionStatus { .. }
        | Commands::CoworkSessionClose { .. }
        | Commands::CoworkHandoff { .. }
        | Commands::CoworkCapture { .. }
        | Commands::MaintenanceRunbook { .. }
        | Commands::Maintenance { .. }
        | Commands::Doctor { .. }
        | Commands::ReleaseReadiness { .. } => unreachable!(),
    }
}

struct SearchCommandArgs<'a> {
    query: &'a str,
    wing: Option<&'a str>,
    room: Option<&'a str>,
    filters: SearchFilters,
    top_k: usize,
    json: bool,
    with_neighbors: bool,
}

struct ContextCommandArgs {
    query: String,
    field: String,
    domain: String,
    cwd: Option<PathBuf>,
    format: String,
    include_evidence: bool,
    include_cards: bool,
    no_include_cards: bool,
    max_items: usize,
    dao_tian_limit: usize,
    no_distill_suggestions: bool,
}

struct BriefCommandArgs {
    query: String,
    field: String,
    domain: String,
    cwd: Option<PathBuf>,
    format: String,
    max_items: usize,
    dao_tian_limit: usize,
}

fn doctor_command(format: &str) -> Result<()> {
    if !matches!(format, "plain" | "json") {
        bail!("unsupported doctor format: {format}");
    }
    let (config, config_error) = match Config::load() {
        Ok(config) => (config, None),
        Err(error) => (Config::default(), Some(error.to_string())),
    };
    let mut report = build_doctor_report(&expand_home(&config.db_path));
    if let Some(error) = config_error {
        report.warnings.push(format!(
            "config could not be loaded; using defaults: {error}"
        ));
    }

    match format {
        "plain" => print_doctor_plain(&report),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&report).context("failed to serialize doctor report")?
        ),
        _ => unreachable!("doctor format was validated"),
    }
    Ok(())
}

fn print_doctor_plain(report: &DoctorReport) {
    println!("mempal doctor");
    println!("current_version={}", report.current_version);
    println!(
        "supported_schema_version={}",
        report.supported_schema_version
    );
    println!("db_path={}", report.db.path);
    println!("db_exists={}", report.db.exists);
    match report.db.schema_version {
        Some(schema_version) => println!("db_schema_version={schema_version}"),
        None => println!("db_schema_version=unknown"),
    }
    println!("db_compatible={}", report.db.compatible);
    if let Some(error) = report.db.error.as_deref() {
        println!("db_error={error}");
    }
    println!(
        "current_exe={}",
        report.install.current_exe.as_deref().unwrap_or("unknown")
    );
    println!(
        "path_mempal={}",
        report.install.path_mempal.as_deref().unwrap_or("not_found")
    );
    match report.install.path_matches_current_exe {
        Some(matches) => println!("path_matches_current_exe={matches}"),
        None => println!("path_matches_current_exe=unknown"),
    }
    for warning in &report.warnings {
        println!("warning={warning}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation={recommendation}");
    }
}

#[derive(Debug, Serialize)]
struct GuidedMaintenanceReport {
    writes: bool,
    drawer_count: u64,
    runtime_adoption_event_count: u64,
    card_count: u64,
    steps: Vec<GuidedMaintenanceStep>,
}

#[derive(Debug, Serialize)]
struct GuidedMaintenanceStep {
    order: usize,
    phase: &'static str,
    purpose: &'static str,
    command: &'static str,
    writes: bool,
}

fn maintenance_guided_run_command(format: &str) -> Result<()> {
    if !matches!(format, "plain" | "json") {
        bail!("unsupported maintenance guided-run format: {format}");
    }
    let config = Config::load().unwrap_or_default();
    let db_path = expand_home(&config.db_path);
    let report = GuidedMaintenanceReport {
        writes: false,
        drawer_count: sqlite_count_if_available(&db_path, "drawers"),
        runtime_adoption_event_count: sqlite_count_if_available(
            &db_path,
            "runtime_adoption_events",
        ),
        card_count: sqlite_count_if_available(&db_path, "knowledge_cards"),
        steps: guided_maintenance_steps(),
    };

    match format {
        "plain" => print_guided_maintenance_plain(&report),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize guided maintenance report")?
        ),
        _ => unreachable!("maintenance guided-run format was validated"),
    }
    Ok(())
}

fn guided_maintenance_steps() -> Vec<GuidedMaintenanceStep> {
    vec![
        GuidedMaintenanceStep {
            order: 1,
            phase: "research",
            purpose: "Validate external research output before any ingest.",
            command: "mempal phase3 research-validate-plan report.json --format json",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 2,
            phase: "research",
            purpose: "Preview evidence drawers and candidate distill suggestions.",
            command: "mempal phase3 research-ingest-plan report.json --format json",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 3,
            phase: "research",
            purpose: "Explicitly ingest validated research findings as evidence only.",
            command: "mempal phase3 research-ingest-plan report.json --execute --format json",
            writes: true,
        },
        GuidedMaintenanceStep {
            order: 4,
            phase: "knowledge",
            purpose: "Create candidate knowledge from explicit evidence refs.",
            command: "mempal knowledge distill --statement ... --content ... --tier dao_ren --supporting-ref drawer_...",
            writes: true,
        },
        GuidedMaintenanceStep {
            order: 5,
            phase: "cards",
            purpose: "Check card lifecycle readiness before promotion.",
            command: "mempal knowledge-card gate card_... --format json",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 6,
            phase: "runtime",
            purpose: "Assemble card-aware context explicitly before changing defaults.",
            command: "mempal context \"current task\" --include-cards --format plain",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 7,
            phase: "runtime",
            purpose: "Review runtime adoption evidence by track and feature.",
            command: "mempal phase3 adoption review --format json",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 8,
            phase: "runtime",
            purpose: "Inspect rollback evidence before keeping card context enabled.",
            command: "mempal phase3 rollback-control card-context --format json",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 9,
            phase: "cowork",
            purpose: "Diagnose concrete agent bus state for the current project.",
            command: "mempal cowork-doctor --cwd \"$PWD\" --format plain",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 10,
            phase: "cowork",
            purpose: "Build a deterministic handoff summary before session close.",
            command: "mempal cowork-handoff --cwd \"$PWD\" --format plain",
            writes: false,
        },
        GuidedMaintenanceStep {
            order: 11,
            phase: "cowork",
            purpose: "Explicitly lift a handoff summary into evidence memory when needed.",
            command: "mempal cowork-capture --cwd \"$PWD\" --summary-source handoff --execute --format json",
            writes: true,
        },
    ]
}

fn print_guided_maintenance_plain(report: &GuidedMaintenanceReport) {
    println!("Guided Maintenance Run");
    println!("writes={}", report.writes);
    println!("drawer_count={}", report.drawer_count);
    println!(
        "runtime_adoption_event_count={}",
        report.runtime_adoption_event_count
    );
    println!("card_count={}", report.card_count);
    for step in &report.steps {
        println!();
        println!("{}. {} ({})", step.order, step.phase, step.purpose);
        println!("command: {}", step.command);
        println!("writes={}", step.writes);
    }
}

fn sqlite_count_if_available(db_path: &Path, table: &str) -> u64 {
    if !db_path.exists() {
        return 0;
    }
    let Ok(conn) =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return 0;
    };
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .ok()
        .and_then(|count| u64::try_from(count).ok())
        .unwrap_or(0)
}

#[derive(Debug, Serialize)]
struct ReleaseReadinessReport {
    writes: bool,
    ready: bool,
    supported_schema_version: u32,
    checks: Vec<ReleaseReadinessCheck>,
    warnings: Vec<String>,
    recommended_commands: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReleaseReadinessCheck {
    name: &'static str,
    passed: bool,
    detail: String,
}

fn release_readiness_command(format: &str) -> Result<()> {
    if !matches!(format, "plain" | "json") {
        bail!("unsupported release-readiness format: {format}");
    }
    let cwd = env::current_dir().context("failed to read current directory")?;
    let report = build_release_readiness_report(&cwd);
    match format {
        "plain" => print_release_readiness_plain(&report),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize release readiness report")?
        ),
        _ => unreachable!("release-readiness format was validated"),
    }
    Ok(())
}

fn build_release_readiness_report(repo_root: &Path) -> ReleaseReadinessReport {
    let cargo_toml = fs::read_to_string(repo_root.join("Cargo.toml")).unwrap_or_default();
    let spec_plan_complete = (98..=104).all(|number| {
        has_file_with_prefix(&repo_root.join("specs"), &format!("p{number}-"))
            && has_file_name_containing(&repo_root.join("docs/plans"), &format!("p{number}"))
    });
    let checks = vec![
        ReleaseReadinessCheck {
            name: "cargo-metadata",
            passed: cargo_toml.contains("name = \"mempal\"")
                && cargo_toml.contains("version = ")
                && cargo_toml.contains("readme = \"README.md\""),
            detail: "Cargo.toml includes package name, version, and README metadata".to_string(),
        },
        ReleaseReadinessCheck {
            name: "readme-docs",
            passed: repo_root.join("README.md").is_file()
                && repo_root.join("README_zh.md").is_file(),
            detail: "README.md and README_zh.md are present".to_string(),
        },
        ReleaseReadinessCheck {
            name: "spec-plan-inventory",
            passed: spec_plan_complete,
            detail: "P98-P104 each have a task spec and matching plan file".to_string(),
        },
        ReleaseReadinessCheck {
            name: "runbooks",
            passed: repo_root.join("docs/COWORK-RUNBOOK.md").is_file()
                && repo_root.join("docs/MAINTENANCE-RUNBOOK.md").is_file(),
            detail: "Cowork and maintenance runbooks are present".to_string(),
        },
        ReleaseReadinessCheck {
            name: "doctor",
            passed: true,
            detail: "mempal doctor is available for install/runtime diagnostics".to_string(),
        },
        ReleaseReadinessCheck {
            name: "schema-support",
            passed: CURRENT_SCHEMA_VERSION == 9,
            detail: format!("current supported schema version is {CURRENT_SCHEMA_VERSION}"),
        },
    ];
    let warnings = checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| format!("release check failed: {}", check.name))
        .collect::<Vec<_>>();
    let ready = warnings.is_empty();
    ReleaseReadinessReport {
        writes: false,
        ready,
        supported_schema_version: CURRENT_SCHEMA_VERSION,
        checks,
        warnings,
        recommended_commands: vec![
            "mempal doctor --format json".to_string(),
            "cargo test".to_string(),
            "cargo clippy -- -D warnings".to_string(),
            "cargo package".to_string(),
        ],
    }
}

fn has_file_with_prefix(dir: &Path, prefix: &str) -> bool {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
}

fn has_file_name_containing(dir: &Path, needle: &str) -> bool {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.file_name().to_string_lossy().contains(needle))
}

fn print_release_readiness_plain(report: &ReleaseReadinessReport) {
    println!("Release Readiness");
    println!("writes={}", report.writes);
    println!("ready={}", report.ready);
    println!(
        "supported_schema_version={}",
        report.supported_schema_version
    );
    for check in &report.checks {
        println!(
            "check={} passed={} detail={}",
            check.name, check.passed, check.detail
        );
    }
    for warning in &report.warnings {
        println!("warning={warning}");
    }
    println!("recommended_commands:");
    for command in &report.recommended_commands {
        println!("- {command}");
    }
}

async fn bench_command(config: &Config, command: BenchCommands) -> Result<()> {
    match command {
        BenchCommands::LongMemEval {
            data_file,
            mode,
            granularity,
            limit,
            skip,
            top_k,
            out,
        } => {
            longmemeval::run_longmemeval_command(
                config,
                LongMemEvalArgs {
                    data_file,
                    mode,
                    granularity,
                    limit,
                    skip,
                    top_k,
                    out,
                },
            )
            .await
        }
    }
}

fn init_command(
    db: &Database,
    dir: &Path,
    dry_run: bool,
    filter_options: ProjectPathFilterOptions,
) -> Result<()> {
    let wing = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
        .to_string();
    let rooms = detect_rooms_with_options(dir, &filter_options)?;

    if !dry_run {
        for room in &rooms {
            let keywords = serde_json::to_string(&vec![room.clone()])
                .context("failed to serialize taxonomy keywords")?;
            db.conn()
                .execute(
                    "INSERT OR IGNORE INTO taxonomy (wing, room, display_name, keywords) VALUES (?1, ?2, ?3, ?4)",
                    (&wing, room, room, keywords.as_str()),
                )
                .with_context(|| format!("failed to insert taxonomy room {room}"))?;
        }
    }

    println!("dry_run={dry_run}");
    println!("wing: {wing}");
    if rooms.is_empty() {
        println!("rooms: none detected");
    } else {
        println!("rooms:");
        for room in rooms {
            println!("- {room}");
        }
    }

    Ok(())
}

async fn ingest_command(db: &Database, config: &Config, args: IngestCommandArgs<'_>) -> Result<()> {
    if let Some(format) = args.format.as_deref()
        && format != "convos"
    {
        bail!("unsupported --format value: {format}");
    }

    let options = IngestOptions {
        room: args.room,
        source_root: if args.dir.is_file() {
            args.dir.parent()
        } else {
            Some(args.dir)
        },
        dry_run: args.dry_run,
        source_file_override: None,
        replace_existing_source: false,
        replace_across_rooms: false,
        no_strip_noise: args.no_strip_noise,
        diary_rollup: args.diary_rollup,
        diary_rollup_day: None,
        project_filter: args.project_filter,
    };

    let stats = if args.dry_run {
        ingest_path_with_options(db, &NoopEmbedder, args.dir, args.wing, options).await?
    } else {
        let embedder = build_embedder(config).await?;
        ingest_path_with_options(db, &*embedder, args.dir, args.wing, options).await?
    };

    append_ingest_audit_log(
        db,
        args.dir,
        args.wing,
        args.format.as_deref(),
        args.dry_run,
        stats,
    )
    .context("failed to append ingest audit log")?;

    println!(
        "dry_run={} files={} chunks={} skipped={} noise_bytes_stripped={} lock_wait_ms={}",
        args.dry_run,
        stats.files,
        stats.chunks,
        stats.skipped,
        stats.noise_bytes_stripped.unwrap_or(0),
        stats.lock_wait_ms.unwrap_or(0)
    );

    Ok(())
}

struct IngestCommandArgs<'a> {
    dir: &'a Path,
    wing: &'a str,
    room: Option<&'a str>,
    format: Option<String>,
    dry_run: bool,
    no_strip_noise: bool,
    diary_rollup: bool,
    project_filter: ProjectPathFilterOptions,
}

async fn ingest_path_with_options<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    path: &Path,
    wing: &str,
    options: IngestOptions<'_>,
) -> mempal::ingest::Result<IngestStats> {
    if path.is_file() {
        ingest_file_with_options(db, embedder, path, wing, options).await
    } else {
        ingest_dir_with_options(db, embedder, path, wing, options).await
    }
}

#[derive(Default)]
struct NoopEmbedder;

#[async_trait::async_trait]
impl Embedder for NoopEmbedder {
    async fn embed(
        &self,
        _texts: &[&str],
    ) -> std::result::Result<Vec<Vec<f32>>, mempal::embed::EmbedError> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn name(&self) -> &str {
        "noop"
    }
}

fn append_ingest_audit_log(
    db: &Database,
    dir: &Path,
    wing: &str,
    format: Option<&str>,
    dry_run: bool,
    stats: IngestStats,
) -> Result<()> {
    let audit_path = db
        .path()
        .parent()
        .map(|parent| parent.join("audit.jsonl"))
        .unwrap_or_else(|| PathBuf::from("audit.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .with_context(|| format!("failed to open audit log {}", audit_path.display()))?;
    let entry = serde_json::json!({
        "timestamp": current_timestamp(),
        "command": "ingest",
        "wing": wing,
        "dir": dir.to_string_lossy(),
        "format": format,
        "dry_run": dry_run,
        "files": stats.files,
        "chunks": stats.chunks,
        "skipped": stats.skipped,
    });
    writeln!(file, "{entry}")
        .with_context(|| format!("failed to write audit log {}", audit_path.display()))?;
    Ok(())
}

async fn context_command(db: &Database, config: &Config, args: ContextCommandArgs) -> Result<()> {
    if args.max_items == 0 {
        bail!("--max-items must be greater than 0");
    }
    let domain = parse_domain(&args.domain)?;
    let cwd = match args.cwd {
        Some(cwd) => cwd,
        None => env::current_dir().context("failed to read current directory")?,
    };

    let embedder = build_embedder(config).await?;
    let include_cards = if args.no_include_cards {
        false
    } else {
        args.include_cards || config.context.include_cards_default
    };
    let pack = assemble_context(
        db,
        &*embedder,
        ContextRequest {
            query: args.query,
            domain,
            field: args.field,
            cwd,
            include_evidence: args.include_evidence,
            include_cards,
            max_items: args.max_items,
            dao_tian_limit: args.dao_tian_limit,
            include_distill_suggestions: !args.no_distill_suggestions,
        },
    )
    .await?;

    match args.format.as_str() {
        "plain" => print_context_plain(&pack),
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&pack).context("failed to serialize context pack")?
            );
        }
        other => bail!("unsupported context format: {other}"),
    }

    Ok(())
}

fn projects_command(db: &Database, format: &str) -> Result<()> {
    let projects = mempal::projects::list_projects(db)?;
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&projects)?),
        "plain" => {
            if projects.is_empty() {
                println!("(no projects)");
            }
            for p in &projects {
                println!(
                    "{}\t{}\tdrawers={} (ev={} kn={})\tlast={}",
                    p.wing,
                    p.path.as_deref().unwrap_or("(no worktree path)"),
                    p.total,
                    p.evidence,
                    p.knowledge,
                    p.last_activity
                );
            }
        }
        other => bail!("unknown format: {other}"),
    }
    Ok(())
}

fn resume_command(
    db: &Database,
    query: &str,
    evidence_limit: usize,
    candidate_limit: usize,
    format: &str,
) -> Result<()> {
    use mempal::projects::ResumeResolution;

    let resolution = mempal::projects::resume_project(db, query, evidence_limit, candidate_limit)?;
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&resolution)?);
            Ok(())
        }
        "plain" => {
            match &resolution {
                ResumeResolution::Resolved(pack) => {
                    println!("resolved: {}", pack.wing);
                    println!("path: {}", pack.path.as_deref().unwrap_or("(unknown)"));
                    println!(
                        "drawers: {} (evidence={} knowledge={}) last={}",
                        pack.total, pack.evidence, pack.knowledge, pack.last_activity
                    );
                    if !pack.recent_evidence.is_empty() {
                        println!("recent evidence:");
                        for e in &pack.recent_evidence {
                            println!(
                                "  [{}] {} :: {}",
                                e.drawer_id,
                                e.source_file.as_deref().unwrap_or("(no source)"),
                                e.snippet
                            );
                        }
                    }
                    if !pack.in_flight.is_empty() {
                        println!("in-flight candidate knowledge:");
                        for c in &pack.in_flight {
                            println!(
                                "  [{}] {}",
                                c.drawer_id,
                                c.statement.as_deref().unwrap_or("(no statement)")
                            );
                        }
                    }
                    println!("next: {}", pack.next_step);
                }
                ResumeResolution::Ambiguous { query, candidates } => {
                    println!("ambiguous '{query}' — matches:");
                    for c in candidates {
                        println!("  {} ({})", c.wing, c.path.as_deref().unwrap_or("no path"));
                    }
                }
                ResumeResolution::NotFound { query, available } => {
                    println!("not found '{query}'. available projects:");
                    for wing in available {
                        println!("  {wing}");
                    }
                }
            }
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

async fn brief_command(db: &Database, config: &Config, args: BriefCommandArgs) -> Result<()> {
    if args.max_items == 0 {
        bail!("--max-items must be greater than 0");
    }
    if !matches!(args.format.as_str(), "plain" | "json") {
        bail!("unsupported brief format: {}", args.format);
    }
    let domain = parse_domain(&args.domain)?;
    let cwd = match args.cwd {
        Some(cwd) => cwd,
        None => env::current_dir().context("failed to read current directory")?,
    };

    let embedder = build_embedder(config).await?;
    let brief = assemble_brief(
        db,
        &*embedder,
        BriefRequest {
            query: args.query,
            domain,
            field: args.field,
            cwd,
            max_items: args.max_items,
            dao_tian_limit: args.dao_tian_limit,
        },
    )
    .await?;

    match args.format.as_str() {
        "plain" => print_brief_plain(&brief),
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&brief)
                    .context("failed to serialize cognitive brief")?
            );
        }
        _ => unreachable!("brief format was validated before embedding"),
    }

    Ok(())
}

fn parse_domain(value: &str) -> Result<MemoryDomain> {
    match value {
        "project" => Ok(MemoryDomain::Project),
        "agent" => Ok(MemoryDomain::Agent),
        "skill" => Ok(MemoryDomain::Skill),
        "global" => Ok(MemoryDomain::Global),
        other => bail!("unsupported domain: {other}"),
    }
}

fn print_context_plain(pack: &ContextPack) {
    if pack.sections.is_empty() {
        println!("no context");
    }

    for section in &pack.sections {
        println!("## {}", section.name);
        for item in &section.items {
            println!("- {}", item.text);
            println!("  source: {}", item.source_file);
            println!("  drawer: {}", item.drawer_id);
            if let Some(card_id) = item.card_id.as_deref() {
                println!("  card: {card_id}");
            }
            println!(
                "  anchor: {} {}",
                anchor_kind_slug(&item.anchor_kind),
                item.anchor_id
            );
            if let (Some(tier), Some(status)) = (&item.tier, &item.status) {
                println!(
                    "  knowledge: tier={} status={}",
                    knowledge_tier_slug(tier),
                    knowledge_status_slug(status)
                );
            }
            if let Some(trigger_hints) = item.trigger_hints.as_ref() {
                println!(
                    "  trigger_hints: intent_tags={} workflow_bias={} tool_needs={}",
                    trigger_hints.intent_tags.join(","),
                    trigger_hints.workflow_bias.join(","),
                    trigger_hints.tool_needs.join(",")
                );
            }
            for citation in &item.evidence_citations {
                println!(
                    "  evidence: {} role={} source={}",
                    citation.evidence_drawer_id,
                    knowledge_evidence_role_slug(&citation.role),
                    citation.source_file
                );
            }
        }
        println!();
    }

    if !pack.distill_suggestions.is_empty() {
        println!("## distill_suggestions");
        for suggestion in &pack.distill_suggestions {
            println!(
                "- field={} evidence_count={} suggested_tier={}",
                suggestion.field, suggestion.evidence_count, suggestion.suggested_tier
            );
            if !suggestion.sample_evidence_drawer_ids.is_empty() {
                println!(
                    "  samples: {}",
                    suggestion.sample_evidence_drawer_ids.join(",")
                );
            }
        }
    }
}

async fn search_command(db: &Database, config: &Config, args: SearchCommandArgs<'_>) -> Result<()> {
    let embedder = build_embedder(config).await?;
    let results = search_with_options(
        db,
        &*embedder,
        args.query,
        args.wing,
        args.room,
        SearchOptions {
            filters: args.filters,
            with_neighbors: args.with_neighbors,
        },
        args.top_k,
    )
    .await?;
    let results = results
        .into_iter()
        .map(build_cli_search_result)
        .collect::<Vec<_>>();

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&results).context("failed to serialize search results")?
        );
        return Ok(());
    }

    if results.is_empty() {
        println!("no results");
        return Ok(());
    }

    for result in results {
        let room = result.room.clone().unwrap_or_else(|| "default".to_string());
        let source_file = result.source_file;
        println!(
            "[{:.3}] {}/{} {}",
            result.similarity, result.wing, room, result.drawer_id
        );
        println!("source: {source_file}");
        println!(
            "kind: {} domain: {} field: {} anchor: {} {}",
            result.memory_kind, result.domain, result.field, result.anchor_kind, result.anchor_id
        );
        if let Some(parent_anchor_id) = result.parent_anchor_id.as_deref() {
            println!("parent_anchor: {parent_anchor_id}");
        }
        if let Some(tier) = result.tier.as_deref() {
            let status = result.status.as_deref().unwrap_or("unknown");
            println!("knowledge: tier={tier} status={status}");
        }
        if let Some(statement) = result.statement.as_deref() {
            println!("statement: {statement}");
        }
        if !result.tunnel_hints.is_empty() {
            println!("tunnel: also in {}", result.tunnel_hints.join(", "));
        }
        if let Some(neighbors) = result.neighbors.as_ref() {
            if let Some(prev) = neighbors.prev.as_ref() {
                println!("prev[{}]: {}", prev.chunk_index, prev.content);
            }
            if let Some(next) = neighbors.next.as_ref() {
                println!("next[{}]: {}", next.chunk_index, next.content);
            }
        }
        println!("{}", result.content);
        println!();
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct CliSearchResult {
    drawer_id: String,
    content: String,
    wing: String,
    room: Option<String>,
    source_file: String,
    similarity: f32,
    route: mempal::core::types::RouteDecision,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tunnel_hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    neighbors: Option<mempal::core::types::ChunkNeighbors>,
    memory_kind: String,
    domain: String,
    field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    statement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    anchor_kind: String,
    anchor_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_anchor_id: Option<String>,
}

fn build_cli_search_result(result: mempal::core::types::SearchResult) -> CliSearchResult {
    CliSearchResult {
        drawer_id: result.drawer_id,
        content: result.content,
        wing: result.wing,
        room: result.room,
        source_file: result.source_file,
        similarity: result.similarity,
        route: result.route,
        tunnel_hints: result.tunnel_hints,
        neighbors: result.neighbors,
        memory_kind: memory_kind_slug(&result.memory_kind).to_string(),
        domain: domain_slug(&result.domain).to_string(),
        field: result.field,
        statement: result.statement,
        tier: result
            .tier
            .as_ref()
            .map(knowledge_tier_slug)
            .map(str::to_string),
        status: result
            .status
            .as_ref()
            .map(knowledge_status_slug)
            .map(str::to_string),
        anchor_kind: anchor_kind_slug(&result.anchor_kind).to_string(),
        anchor_id: result.anchor_id,
        parent_anchor_id: result.parent_anchor_id,
    }
}

fn print_brief_plain(brief: &CognitiveBrief) {
    println!("## Summary");
    println!("{}", brief.summary.narrative);

    println!("## Key Facts");
    if brief.key_facts.is_empty() {
        println!("- none");
    } else {
        for fact in &brief.key_facts {
            println!("- {}", fact.text);
            println!("  drawer: {}", fact.citation.drawer_id);
            println!("  source: {}", fact.citation.source_file);
        }
    }

    println!("## Evidence");
    if brief.evidence.is_empty() {
        println!("- none");
    } else {
        for evidence in &brief.evidence {
            println!("- {}", evidence.text);
            println!("  drawer: {}", evidence.citation.drawer_id);
            println!("  source: {}", evidence.citation.source_file);
        }
    }

    println!("## Cards");
    if brief.cards.is_empty() {
        println!("- none");
    } else {
        for card in &brief.cards {
            println!("- {}", card.text);
            println!("  card: {}", card.card_id);
            println!("  drawer: {}", card.citation.drawer_id);
            println!("  source: {}", card.citation.source_file);
            for citation in &card.evidence_citations {
                println!(
                    "  evidence: {} role={}",
                    citation.evidence_drawer_id,
                    knowledge_evidence_role_slug(&citation.role)
                );
            }
        }
    }

    println!("## Entities");
    if brief.entities.is_empty() {
        println!("- none");
    } else {
        for entity in &brief.entities {
            println!("- {entity}");
        }
    }

    println!("## Unresolved");
    if brief.unresolved_items.is_empty() {
        println!("- none");
    } else {
        for item in &brief.unresolved_items {
            println!("- {}", item.text);
            println!("  drawer: {}", item.citation.drawer_id);
            println!("  source: {}", item.citation.source_file);
        }
    }

    println!("## Uncertainty");
    if brief.uncertainty.is_empty() {
        println!("- none");
    } else {
        for item in &brief.uncertainty {
            println!("- {}: {}", item.kind, item.message);
            for citation in &item.citations {
                println!(
                    "  citation: {} {}",
                    citation.drawer_id, citation.source_file
                );
            }
        }
    }

    println!("## Next Actions");
    if brief.next_actions.is_empty() {
        println!("- none");
    } else {
        for action in &brief.next_actions {
            println!("- {action}");
        }
    }
}

fn memory_kind_slug(value: &MemoryKind) -> &'static str {
    match value {
        MemoryKind::Evidence => "evidence",
        MemoryKind::Knowledge => "knowledge",
    }
}

fn domain_slug(value: &MemoryDomain) -> &'static str {
    match value {
        MemoryDomain::Project => "project",
        MemoryDomain::Agent => "agent",
        MemoryDomain::Skill => "skill",
        MemoryDomain::Global => "global",
    }
}

fn knowledge_tier_slug(value: &KnowledgeTier) -> &'static str {
    match value {
        KnowledgeTier::Qi => "qi",
        KnowledgeTier::Shu => "shu",
        KnowledgeTier::DaoRen => "dao_ren",
        KnowledgeTier::DaoTian => "dao_tian",
    }
}

fn parse_knowledge_tier(value: &str) -> Result<KnowledgeTier> {
    match value {
        "qi" => Ok(KnowledgeTier::Qi),
        "shu" => Ok(KnowledgeTier::Shu),
        "dao_ren" => Ok(KnowledgeTier::DaoRen),
        "dao_tian" => Ok(KnowledgeTier::DaoTian),
        other => bail!("unsupported knowledge tier: {other}"),
    }
}

fn knowledge_status_slug(value: &KnowledgeStatus) -> &'static str {
    match value {
        KnowledgeStatus::Candidate => "candidate",
        KnowledgeStatus::Promoted => "promoted",
        KnowledgeStatus::Canonical => "canonical",
        KnowledgeStatus::Demoted => "demoted",
        KnowledgeStatus::Retired => "retired",
    }
}

fn parse_knowledge_status(value: &str) -> Result<KnowledgeStatus> {
    match value {
        "candidate" => Ok(KnowledgeStatus::Candidate),
        "promoted" => Ok(KnowledgeStatus::Promoted),
        "canonical" => Ok(KnowledgeStatus::Canonical),
        "demoted" => Ok(KnowledgeStatus::Demoted),
        "retired" => Ok(KnowledgeStatus::Retired),
        other => bail!("unsupported knowledge status: {other}"),
    }
}

fn anchor_kind_slug(value: &AnchorKind) -> &'static str {
    match value {
        AnchorKind::Global => "global",
        AnchorKind::Repo => "repo",
        AnchorKind::Worktree => "worktree",
    }
}

fn parse_anchor_kind(value: &str) -> Result<AnchorKind> {
    match value {
        "global" => Ok(AnchorKind::Global),
        "repo" => Ok(AnchorKind::Repo),
        "worktree" => Ok(AnchorKind::Worktree),
        other => bail!("unsupported anchor kind: {other}"),
    }
}

fn knowledge_evidence_role_slug(value: &KnowledgeEvidenceRole) -> &'static str {
    match value {
        KnowledgeEvidenceRole::Supporting => "supporting",
        KnowledgeEvidenceRole::Verification => "verification",
        KnowledgeEvidenceRole::Counterexample => "counterexample",
        KnowledgeEvidenceRole::Teaching => "teaching",
    }
}

fn parse_knowledge_evidence_role(value: &str) -> Result<KnowledgeEvidenceRole> {
    match value {
        "supporting" => Ok(KnowledgeEvidenceRole::Supporting),
        "verification" => Ok(KnowledgeEvidenceRole::Verification),
        "counterexample" => Ok(KnowledgeEvidenceRole::Counterexample),
        "teaching" => Ok(KnowledgeEvidenceRole::Teaching),
        other => bail!("unsupported knowledge evidence role: {other}"),
    }
}

fn knowledge_event_type_slug(value: &KnowledgeEventType) -> &'static str {
    match value {
        KnowledgeEventType::Created => "created",
        KnowledgeEventType::Promoted => "promoted",
        KnowledgeEventType::Demoted => "demoted",
        KnowledgeEventType::Retired => "retired",
        KnowledgeEventType::Linked => "linked",
        KnowledgeEventType::Unlinked => "unlinked",
        KnowledgeEventType::Updated => "updated",
        KnowledgeEventType::PublishedAnchor => "published_anchor",
    }
}

fn parse_knowledge_event_type(value: &str) -> Result<KnowledgeEventType> {
    match value {
        "created" => Ok(KnowledgeEventType::Created),
        "promoted" => Ok(KnowledgeEventType::Promoted),
        "demoted" => Ok(KnowledgeEventType::Demoted),
        "retired" => Ok(KnowledgeEventType::Retired),
        "linked" => Ok(KnowledgeEventType::Linked),
        "unlinked" => Ok(KnowledgeEventType::Unlinked),
        "updated" => Ok(KnowledgeEventType::Updated),
        "published_anchor" => Ok(KnowledgeEventType::PublishedAnchor),
        other => bail!("unsupported knowledge event type: {other}"),
    }
}

fn runtime_adoption_track_slug(value: &RuntimeAdoptionTrack) -> &'static str {
    match value {
        RuntimeAdoptionTrack::RuntimeAdoption => "runtime_adoption",
        RuntimeAdoptionTrack::CardContext => "card_context",
        RuntimeAdoptionTrack::CardEmbedding => "card_embedding",
        RuntimeAdoptionTrack::Evaluator => "evaluator",
        RuntimeAdoptionTrack::ResearchAdapter => "research_adapter",
    }
}

fn parse_runtime_adoption_track(value: &str) -> Result<RuntimeAdoptionTrack> {
    match value {
        "runtime_adoption" => Ok(RuntimeAdoptionTrack::RuntimeAdoption),
        "card_context" => Ok(RuntimeAdoptionTrack::CardContext),
        "card_embedding" => Ok(RuntimeAdoptionTrack::CardEmbedding),
        "evaluator" => Ok(RuntimeAdoptionTrack::Evaluator),
        "research_adapter" => Ok(RuntimeAdoptionTrack::ResearchAdapter),
        other => bail!("unsupported runtime adoption track: {other}"),
    }
}

fn runtime_adoption_signal_slug(value: &RuntimeAdoptionSignal) -> &'static str {
    match value {
        RuntimeAdoptionSignal::Used => "used",
        RuntimeAdoptionSignal::Accepted => "accepted",
        RuntimeAdoptionSignal::Rejected => "rejected",
        RuntimeAdoptionSignal::Miss => "miss",
        RuntimeAdoptionSignal::Rollback => "rollback",
        RuntimeAdoptionSignal::Contradiction => "contradiction",
        RuntimeAdoptionSignal::Neutral => "neutral",
    }
}

fn parse_runtime_adoption_signal(value: &str) -> Result<RuntimeAdoptionSignal> {
    match value {
        "used" => Ok(RuntimeAdoptionSignal::Used),
        "accepted" => Ok(RuntimeAdoptionSignal::Accepted),
        "rejected" => Ok(RuntimeAdoptionSignal::Rejected),
        "miss" => Ok(RuntimeAdoptionSignal::Miss),
        "rollback" => Ok(RuntimeAdoptionSignal::Rollback),
        "contradiction" => Ok(RuntimeAdoptionSignal::Contradiction),
        "neutral" => Ok(RuntimeAdoptionSignal::Neutral),
        other => bail!("unsupported runtime adoption signal: {other}"),
    }
}

fn stable_cli_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update([0]);
        hasher.update(part.trim().as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!("{prefix}_{}", &digest[..16])
}

fn effective_wake_up_text(drawer: &mempal::core::types::Drawer) -> &str {
    match drawer.memory_kind {
        MemoryKind::Knowledge => drawer
            .statement
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(drawer.content.as_str()),
        MemoryKind::Evidence => drawer.content.as_str(),
    }
}

fn wake_up_command(db: &Database, format: Option<&str>) -> Result<()> {
    if let Some("aaak") = format {
        return wake_up_aaak_command(db);
    }
    if let Some("protocol") = format {
        println!("{MEMORY_PROTOCOL}");
        return Ok(());
    }
    if let Some(format) = format {
        bail!("unsupported wake-up format: {format}");
    }

    let drawer_count = db.drawer_count().context("failed to count drawers")?;
    let taxonomy_count = db.taxonomy_count().context("failed to count taxonomy")?;
    let top_drawers = db
        .top_drawers(5)
        .context("failed to load recent drawers for wake-up")?;
    let token_estimate = estimate_wake_up_tokens(&top_drawers);

    // L0: identity + global stats
    println!("## L0 — Identity");
    let identity = read_identity_file();
    if identity.is_empty() {
        println!("{DEFAULT_IDENTITY_HINT}");
    } else {
        for line in identity.lines() {
            println!("{line}");
        }
    }
    println!();
    println!("drawer_count: {drawer_count}");
    println!("taxonomy_entries: {taxonomy_count}");

    // L1: recent context
    println!();
    println!("## L1 — Recent Context");
    if top_drawers.is_empty() {
        println!("no recent drawers");
    } else {
        for drawer in &top_drawers {
            println!(
                "- {}/{} {}",
                drawer.wing,
                render_room(drawer.room.as_deref()),
                drawer.id
            );
            if let Some(source_file) = drawer.source_file.as_deref() {
                println!("  source: {source_file}");
            }
            println!(
                "  {}",
                truncate_for_summary(effective_wake_up_text(drawer), 120)
            );
        }
    }
    println!();
    println!("estimated_tokens: {token_estimate}");

    // Memory protocol (for AI agents reading this output)
    println!();
    println!("## Memory Protocol");
    println!("{MEMORY_PROTOCOL}");

    Ok(())
}

fn read_identity_file() -> String {
    let Some(home) = env::var_os("HOME") else {
        return String::new();
    };
    let identity_path = PathBuf::from(home).join(".mempal").join("identity.txt");
    std::fs::read_to_string(&identity_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn wake_up_aaak_command(db: &Database) -> Result<()> {
    let top_drawers = db
        .top_drawers(5)
        .context("failed to load recent drawers for AAAK wake-up")?;
    let text = if top_drawers.is_empty() {
        "mempal wake-up: no recent drawers".to_string()
    } else {
        top_drawers
            .iter()
            .map(effective_wake_up_text)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let wing = top_drawers
        .first()
        .map(|drawer| drawer.wing.as_str())
        .unwrap_or("mempal");
    let room = top_drawers
        .first()
        .and_then(|drawer| drawer.room.as_deref())
        .unwrap_or("default");
    let output = AaakCodec::default().encode(
        &text,
        &AaakMeta {
            wing: wing.to_string(),
            room: room.to_string(),
            date: current_timestamp(),
            source: "wake-up".to_string(),
        },
    );

    println!("{}", output.document);
    Ok(())
}

fn compress_command(text: &str) -> Result<()> {
    let output = AaakCodec::default().encode(
        text,
        &AaakMeta {
            wing: "manual".to_string(),
            room: "compress".to_string(),
            date: current_timestamp(),
            source: "cli".to_string(),
        },
    );

    println!("{}", output.document);
    Ok(())
}

async fn reindex_command(
    db: &Database,
    config: &Config,
    stale: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if stale && force {
        bail!("--stale and --force are mutually exclusive");
    }
    let mode = if force {
        ReindexMode::Force
    } else {
        ReindexMode::Stale
    };
    let options = ReindexOptions { mode, dry_run };

    let report = if dry_run {
        reindex_sources(db, &NoopEmbedder, options)
            .await
            .context("failed to plan reindex")?
    } else {
        let embedder = build_embedder(config).await?;
        println!("embedder: {} ({}d)", embedder.name(), embedder.dimensions());
        reindex_sources(db, &*embedder, options)
            .await
            .context("failed to reindex sources")?
    };

    print_reindex_report(report, dry_run);
    Ok(())
}

fn print_reindex_report(report: ReindexReport, dry_run: bool) {
    if dry_run {
        println!(
            "would reprocess {} drawers from {} sources",
            report.candidate_drawers, report.candidate_sources
        );
        if report.skipped_missing_drawers > 0 {
            println!(
                "would skip {} drawers from {} missing sources",
                report.skipped_missing_drawers, report.skipped_missing_sources
            );
        }
        return;
    }

    println!(
        "reindex complete: processed {} sources, {} drawers selected, {} chunks written, skipped {} existing chunks, skipped {} missing-source drawers",
        report.processed_sources,
        report.candidate_drawers,
        report.reingested_chunks,
        report.skipped_existing_chunks,
        report.skipped_missing_drawers
    );
}

async fn knowledge_command(
    db: &Database,
    config: &Config,
    command: KnowledgeCommands,
) -> Result<()> {
    match command {
        KnowledgeCommands::Distill {
            statement,
            content,
            tier,
            supporting_refs,
            wing,
            room,
            domain,
            field,
            cwd,
            scope_constraints,
            counterexample_refs,
            teaching_refs,
            intent_tags,
            workflow_bias,
            tool_needs,
            importance,
            dry_run,
        } => {
            let trigger_hints = build_trigger_hints(intent_tags, workflow_bias, tool_needs);
            let request = DistillRequest {
                statement,
                content,
                tier,
                supporting_refs,
                wing,
                room,
                domain,
                field,
                cwd,
                scope_constraints,
                counterexample_refs,
                teaching_refs,
                trigger_hints,
                importance,
                dry_run,
            };
            let outcome = match prepare_distill(db, request)? {
                DistillPlan::Done(outcome) => outcome,
                DistillPlan::Create(prepared) => {
                    let embedder = build_embedder(config).await?;
                    let vector = embedder
                        .embed(&[prepared.content.as_str()])
                        .await
                        .context("failed to embed distilled knowledge")?
                        .into_iter()
                        .next()
                        .context("embedder returned no vector")?;
                    commit_distill(db, *prepared, &vector)?
                }
            };

            if outcome.dry_run {
                println!("dry_run=true drawer_id={}", outcome.drawer_id);
                return Ok(());
            }

            println!(
                "drawer_id={} created={}",
                outcome.drawer_id, outcome.created
            );
        }
        KnowledgeCommands::Promote {
            drawer_id,
            status,
            verification_refs,
            reason,
            reviewer,
        } => {
            let outcome = promote_knowledge(
                db,
                PromoteRequest {
                    drawer_id: drawer_id.clone(),
                    status,
                    verification_refs,
                    reason,
                    reviewer,
                    allow_counterexamples: false,
                    enforce_gate: false,
                },
            )?;
            println!(
                "promoted {}: {} -> {}",
                drawer_id, outcome.old_status, outcome.new_status
            );
        }
        KnowledgeCommands::Demote {
            drawer_id,
            status,
            evidence_refs,
            reason,
            reason_type,
        } => {
            let outcome = demote_knowledge(
                db,
                DemoteRequest {
                    drawer_id: drawer_id.clone(),
                    status,
                    evidence_refs,
                    reason,
                    reason_type,
                },
            )?;
            println!(
                "demoted {}: {} -> {}",
                drawer_id, outcome.old_status, outcome.new_status
            );
        }
        KnowledgeCommands::Gate {
            drawer_id,
            target_status,
            reviewer,
            allow_counterexamples,
            format,
        } => {
            let report = evaluate_gate_by_id(
                db,
                &drawer_id,
                target_status.as_deref(),
                reviewer.as_deref(),
                allow_counterexamples,
            )?;
            match format.as_str() {
                "plain" => print_gate_report(&report),
                "json" => println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("failed to serialize gate report")?
                ),
                other => bail!("unsupported gate format: {other}"),
            }
        }
        KnowledgeCommands::Policy { format } => {
            let policy = promotion_policy();
            match format.as_str() {
                "plain" => print_promotion_policy(&policy),
                "json" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&policy)
                            .context("failed to serialize knowledge policy")?
                    );
                }
                other => bail!("unsupported policy format: {other}"),
            }
        }
        KnowledgeCommands::PublishAnchor {
            drawer_id,
            to,
            target_anchor_id,
            cwd,
            reason,
            reviewer,
        } => {
            let outcome = publish_anchor(
                db,
                PublishAnchorRequest {
                    drawer_id: drawer_id.clone(),
                    to,
                    target_anchor_id,
                    cwd,
                    reason,
                    reviewer,
                },
            )?;
            println!(
                "published {}: {}:{} -> {}:{}",
                drawer_id,
                outcome.old_anchor_kind,
                outcome.old_anchor_id,
                outcome.new_anchor_kind,
                outcome.new_anchor_id
            );
        }
    }

    Ok(())
}

async fn knowledge_card_command(
    db: &Database,
    config: &Config,
    command: KnowledgeCardCommands,
) -> Result<()> {
    match command {
        KnowledgeCardCommands::Create {
            id,
            statement,
            content,
            tier,
            status,
            domain,
            field,
            anchor_kind,
            anchor_id,
            parent_anchor_id,
            scope_constraints,
            intent_tags,
            workflow_bias,
            tool_needs,
            format,
        } => {
            let tier = parse_knowledge_tier(&tier)?;
            let status = parse_knowledge_status(&status)?;
            let domain = parse_domain(&domain)?;
            let anchor_kind = parse_anchor_kind(&anchor_kind)?;
            let trigger_hints = build_trigger_hints(intent_tags, workflow_bias, tool_needs);
            let id = id.unwrap_or_else(|| {
                stable_cli_id(
                    "card",
                    &[
                        statement.as_str(),
                        content.as_str(),
                        knowledge_tier_slug(&tier),
                        knowledge_status_slug(&status),
                        domain_slug(&domain),
                        field.as_str(),
                        anchor_kind_slug(&anchor_kind),
                        anchor_id.as_str(),
                    ],
                )
            });
            let now = current_timestamp();
            let card = KnowledgeCard {
                id: id.clone(),
                statement,
                content,
                tier,
                status,
                domain,
                field,
                anchor_kind,
                anchor_id,
                parent_anchor_id,
                scope_constraints,
                trigger_hints,
                created_at: now.clone(),
                updated_at: now,
            };
            db.insert_knowledge_card(&card)
                .context("failed to insert knowledge card")?;
            match format.as_str() {
                "plain" => println!("card_id={id} created=true"),
                "json" => println!(
                    "{}",
                    serde_json::to_string_pretty(&card)
                        .context("failed to serialize knowledge card")?
                ),
                other => bail!("unsupported knowledge-card format: {other}"),
            }
        }
        KnowledgeCardCommands::Get { card_id, format } => {
            let card = db
                .get_knowledge_card(&card_id)
                .context("failed to get knowledge card")?
                .with_context(|| format!("knowledge card not found: {card_id}"))?;
            print_knowledge_card(&card, &format)?;
        }
        KnowledgeCardCommands::List {
            tier,
            status,
            domain,
            field,
            anchor_kind,
            anchor_id,
            format,
        } => {
            let filter = KnowledgeCardFilter {
                tier: tier.as_deref().map(parse_knowledge_tier).transpose()?,
                status: status.as_deref().map(parse_knowledge_status).transpose()?,
                domain: domain.as_deref().map(parse_domain).transpose()?,
                field,
                anchor_kind: anchor_kind.as_deref().map(parse_anchor_kind).transpose()?,
                anchor_id,
            };
            let cards = db
                .list_knowledge_cards(&filter)
                .context("failed to list knowledge cards")?;
            print_knowledge_cards(&cards, &format)?;
        }
        KnowledgeCardCommands::Retrieve {
            query,
            domain,
            field,
            cwd,
            top_k,
            evidence_top_k,
            format,
        } => {
            if top_k == 0 {
                bail!("--top-k must be greater than 0");
            }
            let domain = parse_domain(&domain)?;
            let cwd = cwd.unwrap_or(env::current_dir().context("failed to read current dir")?);
            let embedder = build_embedder(config).await?;
            let results = retrieve_knowledge_cards(
                db,
                &*embedder,
                KnowledgeCardRetrievalRequest {
                    query,
                    domain,
                    field,
                    cwd,
                    top_k,
                    evidence_top_k,
                },
            )
            .await
            .context("failed to retrieve knowledge cards")?;
            print_retrieved_knowledge_cards(&results, &format)?;
        }
        KnowledgeCardCommands::Link {
            card_id,
            evidence_drawer_id,
            role,
            note,
            id,
        } => {
            let role = parse_knowledge_evidence_role(&role)?;
            let id = id.unwrap_or_else(|| {
                stable_cli_id(
                    "link",
                    &[
                        card_id.as_str(),
                        evidence_drawer_id.as_str(),
                        knowledge_evidence_role_slug(&role),
                        note.as_deref().unwrap_or(""),
                    ],
                )
            });
            let link = KnowledgeEvidenceLink {
                id: id.clone(),
                card_id,
                evidence_drawer_id,
                role,
                note,
                created_at: current_timestamp(),
            };
            db.insert_knowledge_evidence_link(&link)
                .context("failed to insert knowledge evidence link")?;
            println!("link_id={id} created=true");
        }
        KnowledgeCardCommands::Event {
            card_id,
            event_type,
            reason,
            from_status,
            to_status,
            actor,
            metadata_json,
            id,
        } => {
            let event_type = parse_knowledge_event_type(&event_type)?;
            let from_status = from_status
                .as_deref()
                .map(parse_knowledge_status)
                .transpose()?;
            let to_status = to_status
                .as_deref()
                .map(parse_knowledge_status)
                .transpose()?;
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let created_at = current_timestamp();
            let id = id.unwrap_or_else(|| {
                stable_cli_id(
                    "event",
                    &[
                        card_id.as_str(),
                        knowledge_event_type_slug(&event_type),
                        reason.as_str(),
                        created_at.as_str(),
                    ],
                )
            });
            let event = KnowledgeCardEvent {
                id: id.clone(),
                card_id,
                event_type,
                from_status,
                to_status,
                reason,
                actor,
                metadata,
                created_at,
            };
            db.append_knowledge_event(&event)
                .context("failed to append knowledge card event")?;
            println!("event_id={id} created=true");
        }
        KnowledgeCardCommands::Events { card_id, format } => {
            let events = db
                .knowledge_events(&card_id)
                .context("failed to list knowledge card events")?;
            print_knowledge_card_events(&events, &format)?;
        }
        KnowledgeCardCommands::Gate {
            card_id,
            target_status,
            reviewer,
            allow_counterexamples,
            format,
        } => {
            let report = evaluate_card_gate_by_id(
                db,
                &card_id,
                target_status.as_deref(),
                reviewer.as_deref(),
                allow_counterexamples,
            )
            .context("failed to evaluate knowledge card gate")?;
            print_knowledge_card_gate_report(&report, &format)?;
        }
        KnowledgeCardCommands::Promote {
            card_id,
            status,
            verification_refs,
            reason,
            reviewer,
            allow_counterexamples,
            enforce_gate,
            format,
        } => {
            let outcome = promote_card(
                db,
                PromoteCardRequest {
                    card_id,
                    status,
                    verification_refs,
                    reason,
                    reviewer,
                    allow_counterexamples,
                    enforce_gate,
                },
            )
            .context("failed to promote knowledge card")?;
            print_knowledge_card_promote_outcome(&outcome, &format)?;
        }
        KnowledgeCardCommands::Demote {
            card_id,
            status,
            evidence_refs,
            reason,
            reason_type,
            format,
        } => {
            let outcome = demote_card(
                db,
                DemoteCardRequest {
                    card_id,
                    status,
                    evidence_refs,
                    reason,
                    reason_type,
                },
            )
            .context("failed to demote knowledge card")?;
            print_knowledge_card_demote_outcome(&outcome, &format)?;
        }
        KnowledgeCardCommands::BackfillPlan {
            tier,
            status,
            domain,
            field,
            anchor_kind,
            anchor_id,
            format,
        } => {
            let filter = KnowledgeCardFilter {
                tier: tier.as_deref().map(parse_knowledge_tier).transpose()?,
                status: status.as_deref().map(parse_knowledge_status).transpose()?,
                domain: domain.as_deref().map(parse_domain).transpose()?,
                field,
                anchor_kind: anchor_kind.as_deref().map(parse_anchor_kind).transpose()?,
                anchor_id,
            };
            let report = build_backfill_report(db, &filter)
                .context("failed to build knowledge card backfill plan")?;
            print_knowledge_card_backfill_report(&report, &format)?;
        }
        KnowledgeCardCommands::BackfillApply {
            tier,
            status,
            domain,
            field,
            anchor_kind,
            anchor_id,
            execute,
            format,
        } => {
            let filter = KnowledgeCardFilter {
                tier: tier.as_deref().map(parse_knowledge_tier).transpose()?,
                status: status.as_deref().map(parse_knowledge_status).transpose()?,
                domain: domain.as_deref().map(parse_domain).transpose()?,
                field,
                anchor_kind: anchor_kind.as_deref().map(parse_anchor_kind).transpose()?,
                anchor_id,
            };
            let result = apply_backfill(db, &filter, KnowledgeCardBackfillApplyOptions { execute })
                .context("failed to apply knowledge card backfill")?;
            print_knowledge_card_backfill_apply_result(&result, &format)?;
        }
    }

    Ok(())
}

async fn phase3_command(db: &Database, config: &Config, command: Phase3Commands) -> Result<()> {
    match command {
        Phase3Commands::Adoption { command } => phase3_adoption_command(db, command),
        Phase3Commands::Evaluator { command } => phase3_evaluator_command(command),
        Phase3Commands::DefaultProposal {
            candidate,
            rollback_criteria,
            format,
        } => {
            let report = phase3_default_proposal(db, &candidate, rollback_criteria)?;
            print_card_context_default_proposal(&report, &format)
        }
        Phase3Commands::DefaultControl {
            candidate,
            enable,
            disable,
            rollback_criteria,
            format,
        } => {
            let report =
                phase3_default_control(db, &candidate, enable, disable, rollback_criteria)?;
            print_phase3_default_control(&report, &format)
        }
        Phase3Commands::RollbackControl {
            candidate,
            execute,
            format,
        } => {
            let report = phase3_rollback_control(db, &candidate, execute)?;
            print_phase3_rollback_control(&report, &format)
        }
        Phase3Commands::Gate { candidate, format } => {
            let report = phase3_gate_report(db, &candidate)?;
            print_phase3_gate_report(&report, &format)
        }
        Phase3Commands::Readiness { candidate, format } => {
            let report = phase3_readiness_report(db, &candidate)?;
            print_phase3_readiness_report(&report, &format)
        }
        Phase3Commands::ResearchValidatePlan { path, format } => {
            let report = validate_research_adapter_plan(&path)?;
            print_research_adapter_plan(&report, &format)
        }
        Phase3Commands::ResearchIngestPlan {
            path,
            execute,
            format,
        } => research_ingest_plan_command(db, config, &path, execute, &format).await,
    }
}

fn phase3_default_proposal(
    db: &Database,
    candidate: &str,
    rollback_criteria: Vec<String>,
) -> Result<CardContextDefaultProposalReport> {
    match candidate {
        "card-context" => {
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: Some(RuntimeAdoptionTrack::CardContext),
                        feature: Some("include_cards".to_string()),
                    },
                    10_000,
                )
                .context("failed to list runtime adoption events")?;
            Ok(card_context_default_proposal(&events, rollback_criteria))
        }
        other => bail!("unsupported phase3 default proposal candidate: {other}"),
    }
}

#[derive(Debug, Serialize)]
struct Phase3DefaultControlReport {
    writes: bool,
    candidate: String,
    requested: String,
    applied: bool,
    include_cards_default: bool,
    proposal: Option<CardContextDefaultProposalReport>,
    reasons: Vec<String>,
}

fn phase3_default_control(
    db: &Database,
    candidate: &str,
    enable: bool,
    disable: bool,
    rollback_criteria: Vec<String>,
) -> Result<Phase3DefaultControlReport> {
    if enable == disable {
        bail!("exactly one of --enable or --disable is required");
    }
    match candidate {
        "card-context" => {
            let mut config = Config::load().context("failed to load config")?;
            if disable {
                config.context.include_cards_default = false;
                config.save_default().context("failed to save config")?;
                return Ok(Phase3DefaultControlReport {
                    writes: true,
                    candidate: candidate.to_string(),
                    requested: "disable".to_string(),
                    applied: true,
                    include_cards_default: false,
                    proposal: None,
                    reasons: vec!["card context default disabled".to_string()],
                });
            }

            let proposal = phase3_default_proposal(db, candidate, rollback_criteria)?;
            if !proposal.proposal_ready {
                return Ok(Phase3DefaultControlReport {
                    writes: false,
                    candidate: candidate.to_string(),
                    requested: "enable".to_string(),
                    applied: false,
                    include_cards_default: config.context.include_cards_default,
                    proposal: Some(proposal),
                    reasons: vec!["default-on proposal is not ready".to_string()],
                });
            }

            config.context.include_cards_default = true;
            config.save_default().context("failed to save config")?;
            Ok(Phase3DefaultControlReport {
                writes: true,
                candidate: candidate.to_string(),
                requested: "enable".to_string(),
                applied: true,
                include_cards_default: true,
                proposal: Some(proposal),
                reasons: vec!["card context default enabled".to_string()],
            })
        }
        other => bail!("unsupported phase3 default-control candidate: {other}"),
    }
}

fn phase3_rollback_control(
    db: &Database,
    candidate: &str,
    execute: bool,
) -> Result<CardContextRollbackControlReport> {
    match candidate {
        "card-context" => {
            let mut config = Config::load().context("failed to load config")?;
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: Some(RuntimeAdoptionTrack::CardContext),
                        feature: Some("include_cards".to_string()),
                    },
                    10_000,
                )
                .context("failed to list runtime adoption events")?;
            let report = card_context_rollback_control(
                &events,
                config.context.include_cards_default,
                execute,
            );
            if report.applied {
                config.context.include_cards_default = false;
                config.save_default().context("failed to save config")?;
            }
            Ok(report)
        }
        other => bail!("unsupported phase3 rollback-control candidate: {other}"),
    }
}

fn phase3_evaluator_command(command: Phase3EvaluatorCommands) -> Result<()> {
    match command {
        Phase3EvaluatorCommands::Advise {
            evaluator_id,
            subject_kind,
            subject_id,
            proposed_action,
            evidence_refs,
            counterexample_refs,
            risk_notes,
            note,
            format,
        } => {
            let report = evaluator_advice(EvaluatorAdviceInput {
                evaluator_id: evaluator_id.unwrap_or_default(),
                subject_kind,
                subject_id,
                proposed_action,
                evidence_refs,
                counterexample_refs,
                risk_notes,
                note,
            })
            .map_err(anyhow::Error::msg)?;
            print_evaluator_advice(&report, &format)
        }
    }
}

fn phase3_adoption_command(db: &Database, command: Phase3AdoptionCommands) -> Result<()> {
    match command {
        Phase3AdoptionCommands::Guidance { format } => {
            print_runtime_adoption_guidance(&runtime_adoption_guidance(), &format)
        }
        Phase3AdoptionCommands::InstrumentationPolicy { format } => {
            print_runtime_adoption_instrumentation_policy(
                &runtime_adoption_instrumentation_policy(),
                &format,
            )
        }
        Phase3AdoptionCommands::PrepareRecord {
            track,
            signal,
            feature,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            format,
        } => {
            let track = parse_runtime_adoption_track(&track)?;
            let signal = parse_runtime_adoption_signal(&signal)?;
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let plan = prepare_runtime_adoption_record(RuntimeAdoptionRecordPlanInput {
                id,
                track: runtime_adoption_track_slug(&track).to_string(),
                signal: runtime_adoption_signal_slug(&signal).to_string(),
                feature,
                query,
                context_hash,
                card_id,
                evaluator_id,
                research_report_id,
                note,
                metadata,
            });
            print_runtime_adoption_record_plan(&plan, &format)
        }
        Phase3AdoptionCommands::Capture {
            surface,
            outcome,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            execute,
            allow_warnings,
            format,
        } => {
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let record_input = capture_runtime_adoption_record_input(RuntimeAdoptionCaptureInput {
                id,
                surface: surface.clone(),
                outcome: outcome.clone(),
                query,
                context_hash,
                card_id,
                evaluator_id,
                research_report_id,
                note,
                metadata,
            })
            .map_err(anyhow::Error::msg)?;
            let mut report =
                prepare_runtime_adoption_capture(surface, outcome, execute, record_input.clone());
            if execute {
                let track = parse_runtime_adoption_track(&record_input.track)?;
                let signal = parse_runtime_adoption_signal(&record_input.signal)?;
                let should_write =
                    should_write_checked_record(&report.record_quality, allow_warnings);
                let event = if should_write {
                    let event = runtime_adoption_event_from_input(record_input, track, signal);
                    db.insert_runtime_adoption_event(&event)
                        .context("failed to insert runtime adoption event")?;
                    Some(event)
                } else {
                    None
                };
                report.writes = event.is_some();
                report.record_checked = Some(RuntimeAdoptionCheckedRecordReport {
                    writes: event.is_some(),
                    blocked: event.is_none(),
                    record_quality: report.record_quality.clone(),
                    event,
                });
            }
            print_runtime_adoption_capture(&report, &format)
        }
        Phase3AdoptionCommands::Wrap {
            surface,
            outcome,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            execute,
            allow_warnings,
            format,
            command,
        } => {
            let report = phase3_adoption_wrap_command(
                db,
                RuntimeAdoptionWrapArgs {
                    surface,
                    outcome,
                    query,
                    context_hash,
                    card_id,
                    evaluator_id,
                    research_report_id,
                    note,
                    metadata_json,
                    id,
                    execute,
                    allow_warnings,
                    command,
                },
            )?;
            let exit_code = report.child_exit_code;
            print_runtime_adoption_wrap(&report, &format)?;
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
            Ok(())
        }
        Phase3AdoptionCommands::CheckRecord {
            track,
            signal,
            feature,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            format,
        } => {
            let track = parse_runtime_adoption_track(&track)?;
            let signal = parse_runtime_adoption_signal(&signal)?;
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let input = RuntimeAdoptionRecordPlanInput {
                id,
                track: runtime_adoption_track_slug(&track).to_string(),
                signal: runtime_adoption_signal_slug(&signal).to_string(),
                feature,
                query,
                context_hash,
                card_id,
                evaluator_id,
                research_report_id,
                note,
                metadata,
            };
            let report = check_runtime_adoption_record(&input);
            print_runtime_adoption_record_quality(&report, &format)
        }
        Phase3AdoptionCommands::RecordChecked {
            track,
            signal,
            feature,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            allow_warnings,
            format,
        } => {
            let track = parse_runtime_adoption_track(&track)?;
            let signal = parse_runtime_adoption_signal(&signal)?;
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let input = RuntimeAdoptionRecordPlanInput {
                id,
                track: runtime_adoption_track_slug(&track).to_string(),
                signal: runtime_adoption_signal_slug(&signal).to_string(),
                feature,
                query,
                context_hash,
                card_id,
                evaluator_id,
                research_report_id,
                note,
                metadata,
            };
            let quality = check_runtime_adoption_record(&input);
            let should_write = should_write_checked_record(&quality, allow_warnings);
            let event = if should_write {
                let event = runtime_adoption_event_from_input(input, track, signal);
                db.insert_runtime_adoption_event(&event)
                    .context("failed to insert runtime adoption event")?;
                Some(event)
            } else {
                None
            };
            let report = RuntimeAdoptionCheckedRecordReport {
                writes: event.is_some(),
                blocked: event.is_none(),
                record_quality: quality,
                event,
            };
            print_runtime_adoption_checked_record(&report, &format)
        }
        Phase3AdoptionCommands::Review {
            track,
            feature,
            signal,
            limit,
            format,
        } => {
            let track = track
                .as_deref()
                .map(parse_runtime_adoption_track)
                .transpose()?;
            let signal = signal
                .as_deref()
                .map(parse_runtime_adoption_signal)
                .transpose()?;
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: track.clone(),
                        feature: feature.clone(),
                    },
                    limit,
                )
                .context("failed to list runtime adoption events")?;
            let report = review_runtime_adoption_events(
                &events,
                RuntimeAdoptionReviewFilters {
                    track: track
                        .as_ref()
                        .map(runtime_adoption_track_slug)
                        .map(str::to_string),
                    feature,
                    signal: signal
                        .as_ref()
                        .map(runtime_adoption_signal_slug)
                        .map(str::to_string),
                    limit,
                },
            );
            print_runtime_adoption_review(&report, &format)
        }
        Phase3AdoptionCommands::Record {
            track,
            signal,
            feature,
            query,
            context_hash,
            card_id,
            evaluator_id,
            research_report_id,
            note,
            metadata_json,
            id,
            format,
        } => {
            let track = parse_runtime_adoption_track(&track)?;
            let signal = parse_runtime_adoption_signal(&signal)?;
            let metadata = metadata_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .context("failed to parse --metadata-json")?;
            let event = runtime_adoption_event_from_input(
                RuntimeAdoptionRecordPlanInput {
                    id,
                    track: runtime_adoption_track_slug(&track).to_string(),
                    signal: runtime_adoption_signal_slug(&signal).to_string(),
                    feature,
                    query,
                    context_hash,
                    card_id,
                    evaluator_id,
                    research_report_id,
                    note,
                    metadata,
                },
                track,
                signal,
            );
            let id = event.id.clone();
            db.insert_runtime_adoption_event(&event)
                .context("failed to insert runtime adoption event")?;
            match format.as_str() {
                "plain" => println!("event_id={id} created=true"),
                "json" => println!(
                    "{}",
                    serde_json::to_string_pretty(&event)
                        .context("failed to serialize adoption event")?
                ),
                other => bail!("unsupported phase3 adoption format: {other}"),
            }
            Ok(())
        }
        Phase3AdoptionCommands::List {
            track,
            feature,
            limit,
            format,
        } => {
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: track
                            .as_deref()
                            .map(parse_runtime_adoption_track)
                            .transpose()?,
                        feature,
                    },
                    limit,
                )
                .context("failed to list runtime adoption events")?;
            print_runtime_adoption_events(&events, &format)
        }
        Phase3AdoptionCommands::Stats {
            track,
            feature,
            format,
        } => {
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: track
                            .as_deref()
                            .map(parse_runtime_adoption_track)
                            .transpose()?,
                        feature,
                    },
                    10_000,
                )
                .context("failed to list runtime adoption events")?;
            let stats = RuntimeAdoptionStats::from_events(&events);
            print_runtime_adoption_stats(&stats, &format)
        }
        Phase3AdoptionCommands::Analytics { format } => {
            let events = db
                .list_runtime_adoption_events(&RuntimeAdoptionFilter::default(), 10_000)
                .context("failed to list runtime adoption events")?;
            let report = build_runtime_adoption_analytics(&events);
            print_runtime_adoption_analytics(&report, &format)
        }
    }
}

struct RuntimeAdoptionWrapArgs {
    surface: String,
    outcome: String,
    query: Option<String>,
    context_hash: Option<String>,
    card_id: Option<String>,
    evaluator_id: Option<String>,
    research_report_id: Option<String>,
    note: Option<String>,
    metadata_json: Option<String>,
    id: Option<String>,
    execute: bool,
    allow_warnings: bool,
    command: Vec<String>,
}

#[derive(Serialize)]
struct RuntimeAdoptionWrapReport {
    writes: bool,
    execute: bool,
    surface: String,
    outcome: String,
    child_exit_code: i32,
    child_stdout: String,
    child_stderr: String,
    capture: RuntimeAdoptionCaptureReport,
}

fn phase3_adoption_wrap_command(
    db: &Database,
    args: RuntimeAdoptionWrapArgs,
) -> Result<RuntimeAdoptionWrapReport> {
    let (program, argv) = args
        .command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("child command is required after `--`"))?;
    let output = ProcessCommand::new(program)
        .args(argv)
        .output()
        .with_context(|| format!("failed to execute child command `{program}`"))?;
    let child_exit_code = output.status.code().unwrap_or(1);
    let outcome = match args.outcome.as_str() {
        "auto" => {
            if child_exit_code == 0 {
                "accepted".to_string()
            } else {
                "rejected".to_string()
            }
        }
        other => other.to_string(),
    };
    let metadata = args
        .metadata_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .context("failed to parse --metadata-json")?;
    let record_input = capture_runtime_adoption_record_input(RuntimeAdoptionCaptureInput {
        id: args.id,
        surface: args.surface.clone(),
        outcome: outcome.clone(),
        query: args.query,
        context_hash: args.context_hash,
        card_id: args.card_id,
        evaluator_id: args.evaluator_id,
        research_report_id: args.research_report_id,
        note: args.note,
        metadata,
    })
    .map_err(anyhow::Error::msg)?;
    let mut capture = prepare_runtime_adoption_capture(
        args.surface.clone(),
        outcome.clone(),
        args.execute,
        record_input.clone(),
    );
    if args.execute {
        let track = parse_runtime_adoption_track(&record_input.track)?;
        let signal = parse_runtime_adoption_signal(&record_input.signal)?;
        let should_write =
            should_write_checked_record(&capture.record_quality, args.allow_warnings);
        let event = if should_write {
            let event = runtime_adoption_event_from_input(record_input, track, signal);
            db.insert_runtime_adoption_event(&event)
                .context("failed to insert runtime adoption event")?;
            Some(event)
        } else {
            None
        };
        capture.writes = event.is_some();
        capture.record_checked = Some(RuntimeAdoptionCheckedRecordReport {
            writes: event.is_some(),
            blocked: event.is_none(),
            record_quality: capture.record_quality.clone(),
            event,
        });
    }

    Ok(RuntimeAdoptionWrapReport {
        writes: capture.writes,
        execute: args.execute,
        surface: args.surface,
        outcome,
        child_exit_code,
        child_stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        child_stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        capture,
    })
}

fn runtime_adoption_event_from_input(
    input: RuntimeAdoptionRecordPlanInput,
    track: RuntimeAdoptionTrack,
    signal: RuntimeAdoptionSignal,
) -> RuntimeAdoptionEvent {
    let created_at = current_timestamp();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let id = input.id.unwrap_or_else(|| {
        stable_cli_id(
            "adoption",
            &[
                runtime_adoption_track_slug(&track),
                runtime_adoption_signal_slug(&signal),
                input.feature.as_str(),
                input.query.as_deref().unwrap_or(""),
                input.context_hash.as_deref().unwrap_or(""),
                input.card_id.as_deref().unwrap_or(""),
                input.evaluator_id.as_deref().unwrap_or(""),
                input.research_report_id.as_deref().unwrap_or(""),
                created_at.as_str(),
                nonce.as_str(),
            ],
        )
    });
    RuntimeAdoptionEvent {
        id,
        track,
        signal,
        feature: input.feature,
        query: input.query,
        context_hash: input.context_hash,
        card_id: input.card_id,
        evaluator_id: input.evaluator_id,
        research_report_id: input.research_report_id,
        note: input.note,
        metadata: input.metadata,
        created_at,
    }
}

fn print_runtime_adoption_checked_record(
    report: &RuntimeAdoptionCheckedRecordReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("blocked={}", report.blocked);
            println!("quality={}", report.record_quality.quality);
            if let Some(event) = report.event.as_ref() {
                println!("event_id={}", event.id);
            }
            for error in &report.record_quality.errors {
                println!("error={error}");
            }
            for warning in &report.record_quality.warnings {
                println!("warning={warning}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption checked record report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_capture(
    report: &RuntimeAdoptionCaptureReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("execute={}", report.execute);
            println!("surface={}", report.surface);
            println!("outcome={}", report.outcome);
            println!("quality={}", report.record_quality.quality);
            if let Some(checked) = report.record_checked.as_ref() {
                println!("blocked={}", checked.blocked);
                if let Some(event) = checked.event.as_ref() {
                    println!("event_id={}", event.id);
                }
            }
            for error in &report.record_quality.errors {
                println!("error={error}");
            }
            for warning in &report.record_quality.warnings {
                println!("warning={warning}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption capture report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_wrap(report: &RuntimeAdoptionWrapReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("execute={}", report.execute);
            println!("surface={}", report.surface);
            println!("outcome={}", report.outcome);
            println!("child_exit_code={}", report.child_exit_code);
            if !report.child_stdout.is_empty() {
                println!("child_stdout={}", report.child_stdout);
            }
            if !report.child_stderr.is_empty() {
                println!("child_stderr={}", report.child_stderr);
            }
            println!("quality={}", report.capture.record_quality.quality);
            if let Some(checked) = report.capture.record_checked.as_ref() {
                println!("blocked={}", checked.blocked);
                if let Some(event) = checked.event.as_ref() {
                    println!("event_id={}", event.id);
                }
            }
            for error in &report.capture.record_quality.errors {
                println!("error={error}");
            }
            for warning in &report.capture.record_quality.warnings {
                println!("warning={warning}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption wrap report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_review(report: &RuntimeAdoptionReviewReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("total={}", report.total);
            println!("conclusion={}", report.conclusion);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            for feature in &report.features {
                println!(
                    "feature={} total={} accepted={} rejected={} misses={} rollbacks={}",
                    feature.feature,
                    feature.stats.total,
                    feature.stats.accepted,
                    feature.stats.rejected,
                    feature.stats.misses,
                    feature.stats.rollbacks
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption review report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_record_quality(
    report: &RuntimeAdoptionRecordQualityReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("valid={}", report.valid);
            println!("quality={}", report.quality);
            for error in &report.errors {
                println!("error={error}");
            }
            for warning in &report.warnings {
                println!("warning={warning}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption record quality report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_record_plan(
    plan: &RuntimeAdoptionRecordPlan,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", plan.writes);
            println!("record_command={}", plan.record_command.join(" "));
            if let Some(action) = plan.record_payload.get("action").and_then(Value::as_str) {
                println!("action={action}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(plan)
                    .context("failed to serialize runtime adoption record plan")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_guidance(guidance: &RuntimeAdoptionGuidance, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("version={}", guidance.version);
            println!("recording_rule={}", guidance.recording_rule);
            println!("required_fields={}", guidance.required_fields.join(","));
            println!("optional_fields={}", guidance.optional_fields.join(","));
            for signal in &guidance.signals {
                println!("signal={} when={}", signal.signal, signal.when);
            }
            for track in &guidance.tracks {
                println!(
                    "track={} when={} feature_examples={}",
                    track.track,
                    track.when,
                    track.feature_examples.join(",")
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(guidance)
                    .context("failed to serialize runtime adoption guidance")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_instrumentation_policy(
    policy: &mempal::core::phase3::RuntimeAdoptionInstrumentationPolicy,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("version={}", policy.version);
            println!("writes={}", policy.writes);
            println!("default_mode={}", policy.default_mode);
            for mode in &policy.allowed_modes {
                println!(
                    "allowed_mode={} requires_execute={} requires_checked_capture={}",
                    mode.mode, mode.requires_execute, mode.requires_checked_capture
                );
            }
            for mode in &policy.forbidden_modes {
                println!("forbidden_mode={mode}");
            }
            for requirement in &policy.requirements {
                println!("requirement={requirement}");
            }
            for requirement in &policy.rollback_requirements {
                println!("rollback_requirement={requirement}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(policy)
                    .context("failed to serialize runtime adoption instrumentation policy")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

#[derive(Debug, Serialize)]
struct RuntimeAdoptionStats {
    total: usize,
    used: usize,
    accepted: usize,
    rejected: usize,
    misses: usize,
    rollbacks: usize,
    contradictions: usize,
    neutral: usize,
}

impl RuntimeAdoptionStats {
    fn from_events(events: &[RuntimeAdoptionEvent]) -> Self {
        let mut stats = Self {
            total: events.len(),
            used: 0,
            accepted: 0,
            rejected: 0,
            misses: 0,
            rollbacks: 0,
            contradictions: 0,
            neutral: 0,
        };
        for event in events {
            match event.signal {
                RuntimeAdoptionSignal::Used => stats.used += 1,
                RuntimeAdoptionSignal::Accepted => stats.accepted += 1,
                RuntimeAdoptionSignal::Rejected => stats.rejected += 1,
                RuntimeAdoptionSignal::Miss => stats.misses += 1,
                RuntimeAdoptionSignal::Rollback => stats.rollbacks += 1,
                RuntimeAdoptionSignal::Contradiction => stats.contradictions += 1,
                RuntimeAdoptionSignal::Neutral => stats.neutral += 1,
            }
        }
        stats
    }
}

#[derive(Debug, Serialize)]
struct Phase3GateReport {
    candidate: String,
    ready: bool,
    required_track: &'static str,
    stats: RuntimeAdoptionStats,
    reasons: Vec<String>,
}

fn phase3_gate_report(db: &Database, candidate: &str) -> Result<Phase3GateReport> {
    let (track, ready_fn): (RuntimeAdoptionTrack, fn(&RuntimeAdoptionStats) -> bool) =
        match candidate {
            "card-context-default" => (RuntimeAdoptionTrack::CardContext, |stats| {
                stats.accepted >= 3 && stats.rollbacks == 0 && stats.rejected <= stats.accepted
            }),
            "card-embeddings" => (RuntimeAdoptionTrack::CardEmbedding, |stats| {
                stats.misses >= 3 && stats.rollbacks == 0
            }),
            "evaluator-api" => (RuntimeAdoptionTrack::Evaluator, |stats| {
                stats.accepted >= 3 && stats.rollbacks == 0 && stats.contradictions == 0
            }),
            "research-adapter" => (RuntimeAdoptionTrack::ResearchAdapter, |stats| {
                stats.accepted >= 1 && stats.contradictions == 0 && stats.rollbacks == 0
            }),
            other => bail!("unsupported phase3 candidate: {other}"),
        };
    let events = db
        .list_runtime_adoption_events(
            &RuntimeAdoptionFilter {
                track: Some(track.clone()),
                feature: None,
            },
            10_000,
        )
        .context("failed to list runtime adoption events")?;
    let stats = RuntimeAdoptionStats::from_events(&events);
    let ready = ready_fn(&stats);
    let mut reasons = Vec::new();
    if ready {
        reasons.push("minimum evidence threshold satisfied".to_string());
    } else {
        reasons.push("minimum evidence threshold not satisfied".to_string());
    }
    if stats.rollbacks > 0 {
        reasons.push("rollback signals block default or authority changes".to_string());
    }
    if stats.contradictions > 0 {
        reasons.push("contradiction signals require review before implementation".to_string());
    }
    Ok(Phase3GateReport {
        candidate: candidate.to_string(),
        ready,
        required_track: runtime_adoption_track_slug(&track),
        stats,
        reasons,
    })
}

fn phase3_readiness_report(db: &Database, candidate: &str) -> Result<Phase3ReadinessReport> {
    match candidate {
        "card-context-default" => {
            let events = db
                .list_runtime_adoption_events(
                    &RuntimeAdoptionFilter {
                        track: Some(RuntimeAdoptionTrack::CardContext),
                        feature: Some("include_cards".to_string()),
                    },
                    10_000,
                )
                .context("failed to list runtime adoption events")?;
            Ok(card_context_default_readiness(&events))
        }
        other => bail!("unsupported phase3 readiness candidate: {other}"),
    }
}

#[derive(Debug, Serialize)]
struct ResearchAdapterPlanReport {
    valid: bool,
    report_id: String,
    title: String,
    source_count: usize,
    finding_count: usize,
    candidate_insight_count: usize,
    errors: Vec<String>,
}

fn validate_research_adapter_plan(path: &Path) -> Result<ResearchAdapterPlanReport> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read research report {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse research report {}", path.display()))?;
    let mut errors = Vec::new();
    let report_id = required_string(&value, "report_id", &mut errors);
    let title = required_string(&value, "title", &mut errors);
    let sources = value
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if sources == 0 {
        errors.push("sources must contain at least one item".to_string());
    }
    let findings = value
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if findings == 0 {
        errors.push("findings must contain at least one item".to_string());
    }
    let candidate_insights = value
        .get("candidate_insights")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    Ok(ResearchAdapterPlanReport {
        valid: errors.is_empty(),
        report_id,
        title,
        source_count: sources,
        finding_count: findings,
        candidate_insight_count: candidate_insights,
        errors,
    })
}

async fn research_ingest_plan_command(
    db: &Database,
    config: &Config,
    path: &Path,
    execute: bool,
    format: &str,
) -> Result<()> {
    let mut report = build_research_ingest_plan(path)?;
    if report.valid && execute {
        let existing = report
            .evidence_drawers
            .iter()
            .filter_map(|plan| {
                db.get_drawer(&plan.drawer_id)
                    .ok()
                    .flatten()
                    .map(|_| plan.drawer_id.clone())
            })
            .collect::<BTreeSet<_>>();
        let pending = report
            .evidence_drawers
            .iter()
            .enumerate()
            .filter(|(_, plan)| !existing.contains(&plan.drawer_id))
            .map(|(index, plan)| (index, plan.drawer_id.clone(), plan.content.clone()))
            .collect::<Vec<_>>();

        let mut created_indices = BTreeSet::new();
        if !pending.is_empty() {
            let embedder = build_embedder(config).await?;
            let content_refs = pending
                .iter()
                .map(|(_, _, content)| content.as_str())
                .collect::<Vec<_>>();
            let vectors = embedder
                .embed(&content_refs)
                .await
                .context("failed to embed research evidence drawers")?;

            for ((index, drawer_id, content), vector) in pending.into_iter().zip(vectors) {
                let source_file = report.evidence_drawers[index].source_file.clone();
                let drawer = research_evidence_drawer(
                    drawer_id.clone(),
                    content,
                    source_file,
                    report.report_id.as_str(),
                    index,
                );
                db.insert_drawer(&drawer).with_context(|| {
                    format!("failed to insert research evidence drawer {drawer_id}")
                })?;
                db.insert_vector(&drawer_id, &vector)
                    .with_context(|| format!("failed to insert vector for {drawer_id}"))?;
                created_indices.insert(index);
            }
        }

        for (index, plan) in report.evidence_drawers.iter_mut().enumerate() {
            plan.created = created_indices.contains(&index);
            plan.skipped = existing.contains(&plan.drawer_id);
        }
        report.created_count = report
            .evidence_drawers
            .iter()
            .filter(|plan| plan.created)
            .count();
        report.skipped_count = report
            .evidence_drawers
            .iter()
            .filter(|plan| plan.skipped)
            .count();
        report.writes = report.created_count > 0;
    }
    print_research_ingest_plan(&report, format)
}

fn build_research_ingest_plan(path: &Path) -> Result<ResearchIngestPlanReport> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read research report {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse research report {}", path.display()))?;
    Ok(build_research_ingest_plan_from_value(&value))
}

fn research_evidence_drawer(
    drawer_id: String,
    content: String,
    source_file: String,
    report_id: &str,
    finding_index: usize,
) -> Drawer {
    Drawer {
        id: drawer_id,
        content,
        wing: "mempal".to_string(),
        room: Some("research".to_string()),
        source_file: Some(source_file),
        source_type: SourceType::Project,
        added_at: current_timestamp(),
        chunk_index: Some(finding_index as i64),
        normalize_version: CURRENT_NORMALIZE_VERSION,
        importance: 3,
        memory_kind: MemoryKind::Evidence,
        domain: MemoryDomain::Project,
        field: "research".to_string(),
        anchor_kind: AnchorKind::Repo,
        anchor_id: anchor::LEGACY_REPO_ANCHOR_ID.to_string(),
        parent_anchor_id: None,
        provenance: Some(Provenance::Research),
        statement: Some(format!(
            "Research finding from {report_id} #{finding_index}"
        )),
        tier: None,
        status: None,
        supporting_refs: Vec::new(),
        counterexample_refs: Vec::new(),
        teaching_refs: Vec::new(),
        verification_refs: Vec::new(),
        scope_constraints: None,
        trigger_hints: None,
    }
}

fn required_string(
    value: &serde_json::Value,
    field: &'static str,
    errors: &mut Vec<String>,
) -> String {
    match value.get(field).and_then(serde_json::Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
        _ => {
            errors.push(format!("{field} is required"));
            String::new()
        }
    }
}

fn print_runtime_adoption_events(events: &[RuntimeAdoptionEvent], format: &str) -> Result<()> {
    match format {
        "plain" => {
            if events.is_empty() {
                println!("no runtime adoption events");
                return Ok(());
            }
            for event in events {
                println!(
                    "{} track={} signal={} feature={} at={}",
                    event.id,
                    runtime_adoption_track_slug(&event.track),
                    runtime_adoption_signal_slug(&event.signal),
                    event.feature,
                    event.created_at
                );
                if let Some(note) = event.note.as_deref() {
                    println!("  note: {note}");
                }
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(events)
                    .context("failed to serialize runtime adoption events")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_stats(stats: &RuntimeAdoptionStats, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("total={}", stats.total);
            println!("used={}", stats.used);
            println!("accepted={}", stats.accepted);
            println!("rejected={}", stats.rejected);
            println!("misses={}", stats.misses);
            println!("rollbacks={}", stats.rollbacks);
            println!("contradictions={}", stats.contradictions);
            println!("neutral={}", stats.neutral);
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(stats)
                    .context("failed to serialize runtime adoption stats")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption format: {other}"),
    }
}

fn print_runtime_adoption_analytics(
    report: &RuntimeAdoptionAnalyticsReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("adoption analytics");
            println!("writes={}", report.writes);
            println!("total_events={}", report.total_events);
            if report.groups.is_empty() {
                println!("groups=none");
            } else {
                for group in &report.groups {
                    println!(
                        "track={} feature={} total={} used={} accepted={} rejected={} misses={} rollbacks={} contradictions={} neutral={} recommendation={}",
                        group.track,
                        group.feature,
                        group.total,
                        group.used,
                        group.accepted,
                        group.rejected,
                        group.misses,
                        group.rollbacks,
                        group.contradictions,
                        group.neutral,
                        group.recommendation
                    );
                }
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize runtime adoption analytics")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 adoption analytics format: {other}"),
    }
}

fn print_phase3_gate_report(report: &Phase3GateReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("candidate={}", report.candidate);
            println!("ready={}", report.ready);
            println!("required_track={}", report.required_track);
            println!("accepted={}", report.stats.accepted);
            println!("misses={}", report.stats.misses);
            println!("rollbacks={}", report.stats.rollbacks);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize phase3 gate report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 gate format: {other}"),
    }
}

fn print_phase3_readiness_report(report: &Phase3ReadinessReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("candidate={}", report.candidate);
            println!("ready={}", report.ready);
            println!("decision={}", report.decision);
            println!("required_track={}", report.required_track);
            println!("required_feature={}", report.required_feature);
            println!("accepted={}", report.review.stats.accepted);
            println!("rejected={}", report.review.stats.rejected);
            println!("misses={}", report.review.stats.misses);
            println!("rollbacks={}", report.review.stats.rollbacks);
            println!("contradictions={}", report.review.stats.contradictions);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize phase3 readiness report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 readiness format: {other}"),
    }
}

fn print_evaluator_advice(report: &EvaluatorAdviceReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("evaluator_id={}", report.evaluator_id);
            println!("subject_kind={}", report.subject_kind);
            println!("subject_id={}", report.subject_id);
            println!("proposed_action={}", report.proposed_action);
            println!("recommendation={}", report.recommendation);
            println!("lifecycle_authority={}", report.lifecycle_authority);
            println!(
                "deterministic_gate_required={}",
                report.deterministic_gate_required
            );
            println!("requires_human_review={}", report.requires_human_review);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize evaluator advice")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 evaluator format: {other}"),
    }
}

fn print_card_context_default_proposal(
    report: &CardContextDefaultProposalReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("candidate={}", report.candidate);
            println!("proposal_ready={}", report.proposal_ready);
            println!("decision={}", report.decision);
            println!("readiness_ready={}", report.readiness.ready);
            for criterion in &report.rollback_criteria {
                println!("rollback_criterion={criterion}");
            }
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize card context default proposal")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 default proposal format: {other}"),
    }
}

fn print_phase3_default_control(report: &Phase3DefaultControlReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("candidate={}", report.candidate);
            println!("requested={}", report.requested);
            println!("applied={}", report.applied);
            println!("include_cards_default={}", report.include_cards_default);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize phase3 default control report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 default control format: {other}"),
    }
}

fn print_phase3_rollback_control(
    report: &CardContextRollbackControlReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!("writes={}", report.writes);
            println!("candidate={}", report.candidate);
            println!("execute={}", report.execute);
            println!("rollback_required={}", report.rollback_required);
            println!("applied={}", report.applied);
            println!(
                "include_cards_default_before={}",
                report.include_cards_default_before
            );
            println!(
                "include_cards_default_after={}",
                report.include_cards_default_after
            );
            println!("rollback_events={}", report.review.stats.rollbacks);
            for reason in &report.reasons {
                println!("reason={reason}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize phase3 rollback control report")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 rollback-control format: {other}"),
    }
}

fn print_research_adapter_plan(report: &ResearchAdapterPlanReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("valid={}", report.valid);
            println!("report_id={}", report.report_id);
            println!("title={}", report.title);
            println!("source_count={}", report.source_count);
            println!("finding_count={}", report.finding_count);
            println!("candidate_insight_count={}", report.candidate_insight_count);
            for error in &report.errors {
                println!("error={error}");
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize research adapter plan")?
            );
            Ok(())
        }
        other => bail!("unsupported research adapter plan format: {other}"),
    }
}

fn print_research_ingest_plan(report: &ResearchIngestPlanReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("valid={}", report.valid);
            println!("writes={}", report.writes);
            println!("report_id={}", report.report_id);
            println!("title={}", report.title);
            println!("planned_evidence_count={}", report.planned_evidence_count);
            println!("created_count={}", report.created_count);
            println!("skipped_count={}", report.skipped_count);
            println!("candidate_insight_count={}", report.candidate_insight_count);
            for error in &report.errors {
                println!("error={error}");
            }
            for drawer in &report.evidence_drawers {
                println!(
                    "drawer={} finding_index={} created={} skipped={}",
                    drawer.drawer_id, drawer.finding_index, drawer.created, drawer.skipped
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize research ingest plan")?
            );
            Ok(())
        }
        other => bail!("unsupported phase3 research ingest format: {other}"),
    }
}

fn normalized_nonempty_strings(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn build_trigger_hints(
    intent_tags: Vec<String>,
    workflow_bias: Vec<String>,
    tool_needs: Vec<String>,
) -> Option<TriggerHints> {
    let intent_tags = normalized_nonempty_strings(&intent_tags);
    let workflow_bias = normalized_nonempty_strings(&workflow_bias);
    let tool_needs = normalized_nonempty_strings(&tool_needs);
    if intent_tags.is_empty() && workflow_bias.is_empty() && tool_needs.is_empty() {
        return None;
    }
    Some(TriggerHints {
        intent_tags,
        workflow_bias,
        tool_needs,
    })
}

fn print_knowledge_cards(cards: &[KnowledgeCard], format: &str) -> Result<()> {
    match format {
        "plain" => {
            if cards.is_empty() {
                println!("no knowledge cards");
                return Ok(());
            }
            for card in cards {
                println!(
                    "{} tier={} status={} domain={} field={} anchor={} {}",
                    card.id,
                    knowledge_tier_slug(&card.tier),
                    knowledge_status_slug(&card.status),
                    domain_slug(&card.domain),
                    card.field,
                    anchor_kind_slug(&card.anchor_kind),
                    card.anchor_id
                );
                println!("statement: {}", card.statement);
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(cards)
                    .context("failed to serialize knowledge cards")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card format: {other}"),
    }
}

fn print_knowledge_card(card: &KnowledgeCard, format: &str) -> Result<()> {
    match format {
        "plain" => print_knowledge_cards(std::slice::from_ref(card), format),
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(card).context("failed to serialize knowledge card")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card format: {other}"),
    }
}

fn print_retrieved_knowledge_cards(results: &[RetrievedKnowledgeCard], format: &str) -> Result<()> {
    match format {
        "plain" => {
            if results.is_empty() {
                println!("no retrieved knowledge cards");
                return Ok(());
            }
            for result in results {
                let card = &result.card;
                println!(
                    "{} score={:.6} tier={} status={} domain={} field={}",
                    card.id,
                    result.score,
                    knowledge_tier_slug(&card.tier),
                    knowledge_status_slug(&card.status),
                    domain_slug(&card.domain),
                    card.field
                );
                println!("statement: {}", card.statement);
                for citation in &result.evidence_citations {
                    println!(
                        "evidence: {} role={} source={} score={:.6}",
                        citation.evidence_drawer_id,
                        knowledge_evidence_role_slug(&citation.role),
                        citation.source_file,
                        citation.score
                    );
                }
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(results)
                    .context("failed to serialize retrieved knowledge cards")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card retrieve format: {other}"),
    }
}

fn print_knowledge_card_events(events: &[KnowledgeCardEvent], format: &str) -> Result<()> {
    match format {
        "plain" => {
            if events.is_empty() {
                println!("no knowledge card events");
                return Ok(());
            }
            for event in events {
                println!(
                    "{} card_id={} type={} reason={}",
                    event.id,
                    event.card_id,
                    knowledge_event_type_slug(&event.event_type),
                    event.reason
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(events)
                    .context("failed to serialize knowledge card events")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card format: {other}"),
    }
}

fn print_knowledge_card_gate_report(report: &KnowledgeCardGateReport, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!("card_id={}", report.card_id);
            println!("tier={}", report.tier);
            println!("status={}", report.status);
            println!("target_status={}", report.target_status);
            println!("allowed={}", report.allowed);
            println!(
                "evidence_counts supporting={} verification={} teaching={} counterexample={}",
                report.evidence_counts.supporting,
                report.evidence_counts.verification,
                report.evidence_counts.teaching,
                report.evidence_counts.counterexample
            );
            println!(
                "requirements supporting>={} verification>={} teaching>={} reviewer_required={} counterexamples_block={}",
                report.requirements.min_supporting_refs,
                report.requirements.min_verification_refs,
                report.requirements.min_teaching_refs,
                report.requirements.reviewer_required,
                report.requirements.counterexamples_block
            );
            if !report.reasons.is_empty() {
                println!("reasons={}", report.reasons.join("; "));
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize knowledge card gate report")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card gate format: {other}"),
    }
}

fn print_knowledge_card_promote_outcome(outcome: &PromoteCardOutcome, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!(
                "card_id={} old_status={} new_status={} verification_refs={}",
                outcome.card_id,
                outcome.old_status,
                outcome.new_status,
                outcome.verification_refs.join(",")
            );
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(outcome)
                    .context("failed to serialize knowledge card promote outcome")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card promote format: {other}"),
    }
}

fn print_knowledge_card_demote_outcome(outcome: &DemoteCardOutcome, format: &str) -> Result<()> {
    match format {
        "plain" => {
            println!(
                "card_id={} old_status={} new_status={} counterexample_refs={}",
                outcome.card_id,
                outcome.old_status,
                outcome.new_status,
                outcome.counterexample_refs.join(",")
            );
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(outcome)
                    .context("failed to serialize knowledge card demote outcome")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card demote format: {other}"),
    }
}

fn print_knowledge_card_backfill_report(
    report: &KnowledgeCardBackfillReport,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!(
                "ready={} skipped={} already_exists={}",
                report.ready_count, report.skipped_count, report.already_exists_count
            );
            if report.candidates.is_empty() {
                println!("no knowledge drawers");
                return Ok(());
            }
            for candidate in &report.candidates {
                println!(
                    "{} -> {} status={:?}",
                    candidate.source_drawer_id, candidate.prospective_card_id, candidate.status
                );
                if !candidate.reasons.is_empty() {
                    println!("  reasons: {}", candidate.reasons.join("; "));
                }
                if let Some(statement) = candidate.statement.as_deref() {
                    println!("  statement: {statement}");
                }
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(report)
                    .context("failed to serialize knowledge card backfill report")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card backfill-plan format: {other}"),
    }
}

fn print_knowledge_card_backfill_apply_result(
    result: &KnowledgeCardBackfillApplyResult,
    format: &str,
) -> Result<()> {
    match format {
        "plain" => {
            println!(
                "dry_run={} ready={} skipped={} already_exists={} created_count={} linked_count={} event_count={} link_errors={}",
                result.dry_run,
                result.ready_count,
                result.skipped_count,
                result.already_exists_count,
                result.created_count,
                result.linked_count,
                result.event_count,
                result.link_errors.len()
            );
            if result.candidates.is_empty() {
                println!("no knowledge drawers");
            } else {
                for candidate in &result.candidates {
                    println!(
                        "{} -> {} status={:?}",
                        candidate.source_drawer_id, candidate.prospective_card_id, candidate.status
                    );
                    if !candidate.reasons.is_empty() {
                        println!("  reasons: {}", candidate.reasons.join("; "));
                    }
                    if let Some(statement) = candidate.statement.as_deref() {
                        println!("  statement: {statement}");
                    }
                }
            }
            for error in &result.link_errors {
                println!(
                    "link_error card_id={} evidence_drawer_id={} role={} error={}",
                    error.card_id, error.evidence_drawer_id, error.role, error.error
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(result)
                    .context("failed to serialize knowledge card backfill apply result")?
            );
            Ok(())
        }
        other => bail!("unsupported knowledge-card backfill-apply format: {other}"),
    }
}

fn print_gate_report(report: &GateReport) {
    println!("drawer_id={}", report.drawer_id);
    println!("tier={}", report.tier);
    println!("status={}", report.status);
    println!("target_status={}", report.target_status);
    println!("allowed={}", report.allowed);
    println!(
        "evidence_counts supporting={} verification={} teaching={} counterexample={}",
        report.evidence_counts.supporting,
        report.evidence_counts.verification,
        report.evidence_counts.teaching,
        report.evidence_counts.counterexample
    );
    println!(
        "requirements supporting>={} verification>={} teaching>={} reviewer_required={} counterexamples_block={}",
        report.requirements.min_supporting_refs,
        report.requirements.min_verification_refs,
        report.requirements.min_teaching_refs,
        report.requirements.reviewer_required,
        report.requirements.counterexamples_block
    );
    for reason in &report.reasons {
        println!("reason={reason}");
    }
}

fn print_promotion_policy(policy: &[PromotionPolicyEntry]) {
    for entry in policy {
        println!(
            "{} -> {} supporting>={} verification>={} teaching>={} reviewer_required={} counterexamples_block={}",
            entry.tier,
            entry.target_status,
            entry.requirements.min_supporting_refs,
            entry.requirements.min_verification_refs,
            entry.requirements.min_teaching_refs,
            entry.requirements.reviewer_required,
            entry.requirements.counterexamples_block
        );
    }
}

fn delete_command(db: &Database, drawer_id: &str) -> Result<()> {
    // Show what we're about to delete
    let drawer = db
        .get_drawer(drawer_id)
        .context("failed to look up drawer")?;
    match drawer {
        Some(drawer) => {
            db.soft_delete_drawer(drawer_id)
                .context("failed to soft-delete drawer")?;
            append_audit_entry(db, "delete", &serde_json::json!({ "drawer_id": drawer_id }))
                .context("failed to append audit log")?;
            println!("soft-deleted {}", drawer_id);
            println!(
                "  wing={} room={} source={}",
                drawer.wing,
                drawer.room.as_deref().unwrap_or("default"),
                drawer.source_file.as_deref().unwrap_or("(none)")
            );
            println!("  content: {}", truncate_for_summary(&drawer.content, 100));
            println!("  (use `mempal purge` to permanently remove)");
        }
        None => {
            bail!("drawer not found: {drawer_id}");
        }
    }
    Ok(())
}

fn purge_command(db: &Database, before: Option<&str>) -> Result<()> {
    let deleted_count = db
        .deleted_drawer_count()
        .context("failed to count deleted drawers")?;
    if deleted_count == 0 {
        println!("no soft-deleted drawers to purge");
        return Ok(());
    }

    let purged = db
        .purge_deleted(before)
        .context("failed to purge deleted drawers")?;
    append_audit_entry(
        db,
        "purge",
        &serde_json::json!({ "before": before, "purged": purged }),
    )
    .context("failed to append audit log")?;
    println!("permanently removed {purged} drawer(s)");
    Ok(())
}

fn append_audit_entry(db: &Database, command: &str, details: &serde_json::Value) -> Result<()> {
    let audit_path = db
        .path()
        .parent()
        .map(|parent| parent.join("audit.jsonl"))
        .unwrap_or_else(|| PathBuf::from("audit.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .with_context(|| format!("failed to open audit log {}", audit_path.display()))?;
    let entry = serde_json::json!({
        "timestamp": current_timestamp(),
        "command": command,
        "details": details,
    });
    writeln!(file, "{entry}")
        .with_context(|| format!("failed to write audit log {}", audit_path.display()))?;
    Ok(())
}

fn kg_command(db: &Database, command: KgCommands) -> Result<()> {
    use mempal::core::types::Triple;

    match command {
        KgCommands::Add {
            subject,
            predicate,
            object,
            source_drawer,
        } => {
            let id = build_triple_id(&subject, &predicate, &object);
            let triple = Triple {
                id: id.clone(),
                subject: subject.clone(),
                predicate: predicate.clone(),
                object: object.clone(),
                valid_from: Some(current_timestamp()),
                valid_to: None,
                confidence: 1.0,
                source_drawer,
            };
            db.insert_triple(&triple)
                .context("failed to insert triple")?;
            println!("added: ({subject}) --[{predicate}]--> ({object})");
            println!("  id: {id}");
        }
        KgCommands::Query {
            subject,
            predicate,
            object,
            all,
        } => {
            let triples = db
                .query_triples(
                    subject.as_deref(),
                    predicate.as_deref(),
                    object.as_deref(),
                    !all,
                )
                .context("failed to query triples")?;
            if triples.is_empty() {
                println!("no triples found");
            } else {
                for t in &triples {
                    let valid = match (&t.valid_from, &t.valid_to) {
                        (Some(from), Some(to)) => format!("{from}..{to}"),
                        (Some(from), None) => format!("{from}..now"),
                        _ => "always".to_string(),
                    };
                    println!(
                        "({}) --[{}]--> ({})  [{valid}]  id={}",
                        t.subject, t.predicate, t.object, t.id
                    );
                }
                println!("\n{} triple(s)", triples.len());
            }
        }
        KgCommands::Timeline { entity } => {
            let triples = db
                .timeline_for_entity(&entity)
                .context("failed to get timeline")?;
            if triples.is_empty() {
                println!("no triples for '{entity}'");
            } else {
                for t in &triples {
                    let valid = match (&t.valid_from, &t.valid_to) {
                        (Some(from), Some(to)) => format!("{from}..{to}"),
                        (Some(from), None) => format!("{from}..now"),
                        _ => "always".to_string(),
                    };
                    let direction = if t.subject == entity {
                        format!("({}) --[{}]--> ({})", t.subject, t.predicate, t.object)
                    } else {
                        format!("({}) <--[{}]-- ({})", entity, t.predicate, t.subject)
                    };
                    println!("{direction}  [{valid}]");
                }
                println!("\n{} event(s) for '{entity}'", triples.len());
            }
        }
        KgCommands::Stats => {
            let stats = db.triple_stats().context("failed to get KG stats")?;
            println!("total: {}", stats.total);
            println!("active: {}", stats.active);
            println!("expired: {}", stats.expired);
            println!("entities: {}", stats.entities);
            if !stats.top_predicates.is_empty() {
                println!("top predicates:");
                for (pred, count) in &stats.top_predicates {
                    println!("  {pred}: {count}");
                }
            }
        }
        KgCommands::List => {
            let count = db.triple_count().context("failed to count triples")?;
            println!("triple_count: {count}");
        }
    }
    Ok(())
}

fn tunnels_command(db: &Database, command: Option<TunnelCommands>) -> Result<()> {
    match command {
        None => tunnels_discover_command(db),
        Some(TunnelCommands::Add { left, right, label }) => {
            let tunnel = db
                .create_tunnel(
                    &parse_tunnel_endpoint(&left)?,
                    &parse_tunnel_endpoint(&right)?,
                    &label,
                    Some("mempal-cli"),
                )
                .context("failed to add tunnel")?;
            println!(
                "created tunnel {}\n{} <-> {} | {}",
                tunnel.id,
                format_tunnel_endpoint(&tunnel.left),
                format_tunnel_endpoint(&tunnel.right),
                tunnel.label
            );
            Ok(())
        }
        Some(TunnelCommands::List { wing, kind }) => {
            tunnels_list_command(db, wing.as_deref(), &kind)
        }
        Some(TunnelCommands::Delete { tunnel_id }) => {
            if tunnel_id.starts_with("passive_") {
                bail!("cannot delete passive tunnel");
            }
            if db
                .delete_explicit_tunnel(&tunnel_id)
                .context("failed to delete tunnel")?
            {
                println!("deleted tunnel {tunnel_id}");
                Ok(())
            } else {
                bail!("tunnel not found: {tunnel_id}");
            }
        }
        Some(TunnelCommands::Follow { from, hops }) => {
            let endpoint = parse_tunnel_endpoint(&from)?;
            let results = db
                .follow_explicit_tunnels(&endpoint, hops)
                .context("failed to follow tunnels")?;
            if results.is_empty() {
                println!("no explicit tunnel neighbors");
            } else {
                for result in &results {
                    println!(
                        "hop {} via {} -> {}",
                        result.hop,
                        result.via_tunnel_id,
                        format_tunnel_endpoint(&result.endpoint)
                    );
                }
                println!("\n{} tunnel neighbor(s)", results.len());
            }
            Ok(())
        }
    }
}

fn tunnels_discover_command(db: &Database) -> Result<()> {
    let tunnels = db.find_tunnels().context("failed to find tunnels")?;
    if tunnels.is_empty() {
        println!("no tunnels (need rooms shared across multiple wings)");
    } else {
        for (room, wings) in &tunnels {
            println!("room '{}' ↔ wings: {}", room, wings.join(", "));
        }
        println!("\n{} tunnel(s)", tunnels.len());
    }
    Ok(())
}

fn tunnels_list_command(db: &Database, wing: Option<&str>, kind: &str) -> Result<()> {
    let mut count = 0_usize;
    if matches!(kind, "all" | "passive") {
        for (room, wings) in db
            .find_tunnels()
            .context("failed to find passive tunnels")?
        {
            if wing.is_none_or(|filter| wings.iter().any(|item| item == filter)) {
                println!(
                    "passive passive_{room}: room '{room}' ↔ wings: {}",
                    wings.join(", ")
                );
                count += 1;
            }
        }
    }
    if matches!(kind, "all" | "explicit") {
        for tunnel in db
            .list_explicit_tunnels(wing)
            .context("failed to list explicit tunnels")?
        {
            println!(
                "explicit {}: {} <-> {} | {}",
                tunnel.id,
                format_tunnel_endpoint(&tunnel.left),
                format_tunnel_endpoint(&tunnel.right),
                tunnel.label
            );
            count += 1;
        }
    }
    if !matches!(kind, "all" | "passive" | "explicit") {
        bail!("unsupported tunnel kind: {kind}");
    }
    if count == 0 {
        println!("no tunnels");
    } else {
        println!("\n{count} tunnel(s)");
    }
    Ok(())
}

fn parse_tunnel_endpoint(value: &str) -> Result<TunnelEndpoint> {
    let trimmed = value.trim();
    let (wing, room) = match trimmed.split_once(':') {
        Some((wing, room)) => (wing.trim(), Some(room.trim())),
        None => (trimmed, None),
    };
    if wing.is_empty() {
        bail!("tunnel endpoint wing is required");
    }
    Ok(TunnelEndpoint {
        wing: wing.to_string(),
        room: room.filter(|room| !room.is_empty()).map(ToOwned::to_owned),
    })
}

fn taxonomy_command(db: &Database, command: TaxonomyCommands) -> Result<()> {
    match command {
        TaxonomyCommands::List => taxonomy_list_command(db),
        TaxonomyCommands::Edit {
            wing,
            room,
            keywords,
        } => taxonomy_edit_command(db, &wing, &room, &keywords),
    }
}

fn taxonomy_list_command(db: &Database) -> Result<()> {
    let entries = db
        .taxonomy_entries()
        .context("failed to load taxonomy entries")?;

    if entries.is_empty() {
        println!("no taxonomy entries");
        return Ok(());
    }

    for entry in entries {
        let keywords = if entry.keywords.is_empty() {
            "<none>".to_string()
        } else {
            entry.keywords.join(", ")
        };
        println!(
            "- {}/{} [{}]",
            entry.wing,
            render_room(Some(entry.room.as_str())),
            keywords
        );
    }

    Ok(())
}

fn taxonomy_edit_command(db: &Database, wing: &str, room: &str, keywords: &str) -> Result<()> {
    let entry = TaxonomyEntry {
        wing: wing.to_string(),
        room: room.to_string(),
        display_name: Some(room.to_string()),
        keywords: parse_keywords_arg(keywords),
    };
    db.upsert_taxonomy_entry(&entry)
        .context("failed to update taxonomy entry")?;

    println!(
        "updated {}/{} [{}]",
        wing,
        render_room(Some(room)),
        entry.keywords.join(", ")
    );

    Ok(())
}

fn field_taxonomy_command(format: &str) -> Result<()> {
    let entries = field_taxonomy();
    match format {
        "plain" => print_field_taxonomy(&entries),
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&entries)
                    .context("failed to serialize field taxonomy")?
            );
        }
        other => bail!("unsupported field taxonomy format: {other}"),
    }
    Ok(())
}

fn print_field_taxonomy(entries: &[FieldTaxonomyEntry]) {
    for entry in entries {
        println!(
            "- {} domains={} examples={} :: {}",
            entry.field,
            entry.domains.join(","),
            entry.examples.join("; "),
            entry.description
        );
    }
}

fn fact_check_command(
    db: &Database,
    path: Option<&Path>,
    wing: Option<&str>,
    room: Option<&str>,
    now: Option<String>,
) -> Result<()> {
    use std::io::Read;

    let text = match path {
        Some(p) if p.as_os_str() == "-" => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read stdin")?;
            buf
        }
        Some(p) => {
            std::fs::read_to_string(p).with_context(|| format!("failed to read {}", p.display()))?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read stdin")?;
            buf
        }
    };

    let now_secs = mempal::factcheck::resolve_now(now.as_deref())?;
    let scope = mempal::factcheck::validate_scope(wing, room)?;

    let report =
        mempal::factcheck::check(&text, db, now_secs, scope).context("fact check failed")?;

    let json =
        serde_json::to_string_pretty(&report).context("failed to serialize fact-check report")?;
    println!("{json}");
    Ok(())
}

fn status_command(db: &Database) -> Result<()> {
    let schema_version = db
        .schema_version()
        .context("failed to read schema version")?;
    let drawer_count = db.drawer_count().context("failed to count drawers")?;
    let taxonomy_count = db.taxonomy_count().context("failed to count taxonomy")?;
    let db_size_bytes = db
        .database_size_bytes()
        .context("failed to compute database size")?;

    let deleted_count = db
        .deleted_drawer_count()
        .context("failed to count deleted drawers")?;
    let diary_rollup_days = db
        .diary_rollup_days()
        .context("failed to count diary rollup days")?;

    println!("schema_version: {schema_version}");
    println!("drawer_count: {drawer_count}");
    println!("diary_rollup_days: {diary_rollup_days}");
    if deleted_count > 0 {
        println!("deleted_drawers: {deleted_count} (use `mempal purge` to remove)");
    }
    let triple_count = db.triple_count().context("failed to count triples")?;

    println!("taxonomy_entries: {taxonomy_count}");
    if triple_count > 0 {
        println!("triples: {triple_count}");
    }
    println!("db_size_bytes: {db_size_bytes}");

    let counts = db.scope_counts().context("failed to query scope counts")?;

    println!("scopes:");
    if counts.is_empty() {
        println!("- none");
    } else {
        for (wing, room, count) in counts {
            println!("- {wing}/{}: {count}", render_room(room.as_deref()));
        }
    }

    Ok(())
}

async fn serve_command(config: &Config, mcp: bool) -> Result<()> {
    if mcp {
        return serve_mcp_command(config).await;
    }

    #[cfg(feature = "rest")]
    {
        return serve_mcp_and_rest_command(config).await;
    }

    #[cfg(not(feature = "rest"))]
    {
        serve_mcp_command(config).await
    }
}

async fn serve_mcp_command(config: &Config) -> Result<()> {
    let server = MempalMcpServer::new(expand_home(&config.db_path), config.clone());
    let service = server.serve_stdio().await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(feature = "rest")]
async fn serve_mcp_and_rest_command(config: &Config) -> Result<()> {
    let db_path = expand_home(&config.db_path);
    let listener = tokio::net::TcpListener::bind(DEFAULT_REST_ADDR)
        .await
        .with_context(|| format!("failed to bind REST server to {DEFAULT_REST_ADDR}"))?;
    let local_addr = listener
        .local_addr()
        .context("failed to resolve REST server address")?;
    eprintln!("REST listening on http://{local_addr}");

    let state = ApiState::new(
        db_path.clone(),
        Arc::new(ConfiguredEmbedderFactory::new(config.clone())),
    );
    let mut rest_task = tokio::spawn(async move {
        serve_rest_api(listener, state)
            .await
            .context("REST server failed")
    });

    let server = MempalMcpServer::new(db_path, config.clone());
    let service = server.serve_stdio().await?;
    let mut mcp_task = Box::pin(async move {
        service.waiting().await.context("MCP server failed")?;
        Ok(())
    });

    tokio::select! {
        mcp_result = &mut mcp_task => {
            rest_task.abort();
            match rest_task.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return Err(error),
                Err(join_error) if join_error.is_cancelled() => {}
                Err(join_error) => {
                    return Err(anyhow::Error::new(join_error).context("failed to join REST task"));
                }
            }
            mcp_result
        }
        rest_result = &mut rest_task => match rest_result {
            Ok(Ok(())) => bail!("REST server exited unexpectedly"),
            Ok(Err(error)) => Err(error),
            Err(join_error) => Err(anyhow::Error::new(join_error).context("failed to join REST task")),
        },
    }
}

async fn build_embedder(config: &Config) -> Result<Box<dyn Embedder>> {
    use mempal::embed::EmbedderFactory;
    ConfiguredEmbedderFactory::new(config.clone())
        .build()
        .await
        .context("failed to initialize embedder")
}

fn expand_home(path: &str) -> PathBuf {
    expand_home_with_home(path, home_dir())
}

fn expand_home_with_home(path: &str, home: Option<PathBuf>) -> PathBuf {
    if path == "~" {
        return home.unwrap_or_else(|| PathBuf::from("."));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return home
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(".").join(rest));
    }

    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            if drive.is_empty() || path.is_empty() {
                return None;
            }
            let mut home = PathBuf::from(drive);
            home.push(path);
            Some(home)
        })
}

fn cowork_register_command(
    agent_id: String,
    tool: String,
    cwd: PathBuf,
    transport: String,
    tmux_target: Option<String>,
) -> Result<()> {
    use mempal::cowork::bus::{self, RegisterAgentRequest};
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let record = bus::register_agent(
        &mempal_home,
        &cwd,
        RegisterAgentRequest {
            agent_id,
            tool,
            transport,
            tmux_target,
        },
    )?;
    println!(
        "registered agent {} tool={} transport={}",
        record.agent_id, record.tool, record.transport
    );
    if let Some(tmux_target) = record.tmux_target {
        println!("tmux_target={tmux_target}");
    }
    Ok(())
}

fn cowork_send_command(
    from: String,
    to: Vec<String>,
    cwd: PathBuf,
    message: String,
    operation: mempal::cowork::bus::SendOperation,
    thread_id: Option<String>,
    channel: Option<String>,
) -> Result<()> {
    use mempal::cowork::bus::{self, SendRequest};
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let report = bus::send(
        &mempal_home,
        &cwd,
        SendRequest {
            from,
            targets: to,
            message,
            operation,
            thread_id,
            channel,
        },
    )?;
    print_delivery_report(report.delivered);
    Ok(())
}

fn print_delivery_report(deliveries: Vec<mempal::cowork::bus::DeliveryReport>) {
    for delivery in deliveries {
        let thread = delivery
            .thread_id
            .as_ref()
            .map(|thread_id| format!(" thread={thread_id}"))
            .unwrap_or_default();
        let channel = delivery
            .channel
            .as_ref()
            .map(|channel| format!(" channel={channel}"))
            .unwrap_or_default();
        match delivery.transport.as_str() {
            "inbox" => println!(
                "sent to {} message_id={} transport=inbox{}{} inbox={} bytes={}",
                delivery.target_agent_id,
                delivery.message_id,
                thread,
                channel,
                delivery
                    .inbox_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                delivery.inbox_size_after.unwrap_or_default()
            ),
            "tmux" => println!(
                "sent to {} message_id={} transport=tmux{}{} tmux_target={}",
                delivery.target_agent_id,
                delivery.message_id,
                thread,
                channel,
                delivery.tmux_target.unwrap_or_default()
            ),
            other => println!("sent to {} transport={other}", delivery.target_agent_id),
        }
    }
}

fn cowork_agent_drain_command(agent_id: String, cwd: PathBuf, format: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let messages = bus::drain_agent(&mempal_home, &cwd, &agent_id)?;
    if messages.is_empty() {
        return Ok(());
    }
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_agent_plain(&agent_id, &messages));
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_agents_command(cwd: PathBuf, now: Option<String>) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let statuses = bus::list_agent_status_at(&mempal_home, &cwd, now.as_deref())?;
    println!("Project: {}", cwd.display());
    println!();
    if statuses.is_empty() {
        println!("no registered agents");
        return Ok(());
    }
    for status in statuses {
        let record = status.record;
        println!(
            "{}: tool={} transport={} presence={} last_seen_at={} pending={} bytes={}",
            record.agent_id,
            record.tool,
            record.transport,
            status.presence,
            record.last_seen_at.as_deref().unwrap_or("-"),
            status.pending_count,
            status.pending_bytes
        );
        if let Some(tmux_target) = record.tmux_target {
            println!("  tmux_target={tmux_target}");
        }
        for msg in status.preview {
            println!("  from {} @ {}: {}", msg.from, msg.pushed_at, msg.content);
        }
    }
    Ok(())
}

fn cowork_heartbeat_command(agent_id: String, cwd: PathBuf, seen_at: Option<String>) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let record = bus::heartbeat_agent(&mempal_home, &cwd, &agent_id, seen_at.as_deref())?;
    println!(
        "heartbeat agent_id={} last_seen_at={}",
        record.agent_id,
        record.last_seen_at.unwrap_or_default()
    );
    Ok(())
}

fn cowork_channel_set_command(channel: String, agents: Vec<String>, cwd: PathBuf) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let record = bus::set_channel(&mempal_home, &cwd, &channel, agents)?;
    println!(
        "channel {} agents={}",
        record.channel,
        record.agents.join(",")
    );
    Ok(())
}

fn cowork_channel_send_command(
    from: String,
    channel: String,
    cwd: PathBuf,
    message: String,
    thread_id: Option<String>,
) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let report = bus::send_channel(&mempal_home, &cwd, from, channel, message, thread_id)?;
    print_delivery_report(report.delivered);
    Ok(())
}

fn cowork_tmux_peek_command(agent_id: String, cwd: PathBuf, lines: usize) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let peek = bus::tmux_peek_agent(&mempal_home, &cwd, &agent_id, lines)?;
    print!("{}", peek.content);
    Ok(())
}

fn cowork_peek_command(
    tool: String,
    cwd: PathBuf,
    limit: usize,
    since: Option<String>,
    format: String,
) -> Result<()> {
    use mempal::cowork::peek::{PeekRequest, Tool, peek_partner};

    let target = Tool::from_str_ci(&tool)
        .ok_or_else(|| anyhow::anyhow!("unknown tool `{tool}`: expected claude|codex"))?;
    let resp = peek_partner(PeekRequest {
        tool: target,
        limit,
        since,
        cwd,
        // Shell caller has no MCP ClientInfo; self-peek does not apply.
        caller_tool: None,
        home_override: None,
    })
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    match format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        "plain" => {
            println!(
                "partner={} active={} session={}",
                resp.partner_tool.as_str(),
                resp.partner_active,
                resp.session_path.as_deref().unwrap_or("(none)")
            );
            for msg in &resp.messages {
                println!("[{} @ {}] {}", msg.role, msg.at, msg.text);
            }
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn static_runbook_command(title: &str, content: &str, format: &str) -> Result<()> {
    match format {
        "plain" => {
            print!("{content}");
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "title": title,
                    "content": content,
                }))?
            );
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_doctor_command(
    cwd: PathBuf,
    now: Option<String>,
    probe_tmux: bool,
    format: String,
) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let report = bus::doctor(&mempal_home, &cwd, now.as_deref(), probe_tmux)?;
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_doctor_plain(&report));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_session_create_command(
    cwd: PathBuf,
    session_id: String,
    title: String,
    goal: Option<String>,
    agents: Vec<String>,
    channels: Vec<String>,
    thread_id: Option<String>,
) -> Result<()> {
    use mempal::cowork::bus::{self, CreateSessionRequest};
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let session = bus::create_session(
        &mempal_home,
        &cwd,
        CreateSessionRequest {
            session_id,
            title,
            goal,
            agents,
            channels,
            thread_id,
        },
    )?;
    println!(
        "session {} status={} agents={}",
        session.session_id,
        session.status,
        session.agents.join(",")
    );
    Ok(())
}

fn cowork_sessions_command(cwd: PathBuf, format: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let sessions = bus::list_sessions(&mempal_home, &cwd)?;
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_sessions_plain(&sessions));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&sessions)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_session_status_command(cwd: PathBuf, session_id: String, status: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let session = bus::update_session_status(&mempal_home, &cwd, &session_id, &status)?;
    println!("session {} status={}", session.session_id, session.status);
    Ok(())
}

#[derive(Debug, Serialize)]
struct CoworkSessionCloseReport {
    session: mempal::cowork::bus::TeamSession,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture: Option<mempal::cowork::bus::CoworkCaptureReport>,
}

fn cowork_session_close_command(
    cwd: PathBuf,
    session_id: String,
    capture: bool,
    execute: bool,
    format: String,
) -> Result<()> {
    use mempal::cowork::bus::{self, CoworkCaptureRequest};
    use mempal::cowork::inbox;

    if !matches!(format.as_str(), "plain" | "json") {
        bail!("unknown format: {format}");
    }
    let mempal_home = inbox::mempal_home();
    let session = bus::update_session_status(&mempal_home, &cwd, &session_id, "closed")?;
    let capture = if capture {
        let db = if execute {
            let config = Config::load().context("failed to load config")?;
            Some(Database::open(&expand_home(&config.db_path)).context("failed to open database")?)
        } else {
            None
        };
        Some(bus::capture_handoff_to_memory(
            db.as_ref(),
            &mempal_home,
            &cwd,
            CoworkCaptureRequest {
                summary_source: "handoff".to_string(),
                wing: "cowork-capture".to_string(),
                room: None,
                thread_id: None,
                channel: None,
                session_id: Some(session_id),
                note: Some("Session closed through cowork-session-close.".to_string()),
                execute,
            },
        )?)
    } else {
        None
    };
    let report = CoworkSessionCloseReport { session, capture };
    match format.as_str() {
        "plain" => {
            println!(
                "session {} status={}",
                report.session.session_id, report.session.status
            );
            if let Some(capture) = &report.capture {
                print!("{}", bus::format_capture_plain(capture));
            }
        }
        "json" => println!("{}", serde_json::to_string_pretty(&report)?),
        _ => unreachable!("cowork-session-close format was validated"),
    }
    Ok(())
}

fn cowork_handoff_command(
    cwd: PathBuf,
    thread_id: Option<String>,
    channel: Option<String>,
    session_id: Option<String>,
    limit: Option<usize>,
    format: String,
) -> Result<()> {
    use mempal::cowork::bus::{self, HandoffFilters};
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let summary = bus::build_handoff_summary(
        &mempal_home,
        &cwd,
        HandoffFilters {
            thread_id,
            channel,
            session_id,
            limit,
        },
    )?;
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_handoff_plain(&summary));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&summary)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

struct CoworkCaptureCommandArgs {
    cwd: PathBuf,
    summary_source: String,
    wing: String,
    room: Option<String>,
    thread_id: Option<String>,
    channel: Option<String>,
    session_id: Option<String>,
    note: Option<String>,
    execute: bool,
    format: String,
}

fn cowork_capture_command(args: CoworkCaptureCommandArgs) -> Result<()> {
    use mempal::cowork::bus::{self, CoworkCaptureRequest};
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let db = if args.execute {
        let config = Config::load().context("failed to load config")?;
        Some(Database::open(&expand_home(&config.db_path)).context("failed to open database")?)
    } else {
        None
    };
    let report = bus::capture_handoff_to_memory(
        db.as_ref(),
        &mempal_home,
        &args.cwd,
        CoworkCaptureRequest {
            summary_source: args.summary_source,
            wing: args.wing,
            room: args.room,
            thread_id: args.thread_id,
            channel: args.channel,
            session_id: args.session_id,
            note: args.note,
            execute: args.execute,
        },
    )?;
    match args.format.as_str() {
        "plain" => {
            print!("{}", bus::format_capture_plain(&report));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_events_command(cwd: PathBuf, limit: Option<usize>, format: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let events = bus::list_events(&mempal_home, &cwd, limit)?;
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_events_plain(&events));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&events)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

fn cowork_ack_command(agent_id: String, cwd: PathBuf, message_id: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let status = bus::ack_delivery(&mempal_home, &cwd, &agent_id, &message_id)?;
    println!(
        "acked message_id={} agent_id={} status={}",
        status.message_id, status.target_agent_id, status.status
    );
    Ok(())
}

fn cowork_deliveries_command(cwd: PathBuf, agent_id: Option<String>, format: String) -> Result<()> {
    use mempal::cowork::bus;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    let deliveries = bus::list_delivery_statuses(&mempal_home, &cwd, agent_id.as_deref())?;
    match format.as_str() {
        "plain" => {
            print!("{}", bus::format_delivery_statuses_plain(&deliveries));
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&deliveries)?);
            Ok(())
        }
        other => bail!("unknown format: {other}"),
    }
}

/// `mempal cowork-drain` — called by UserPromptSubmit hooks. Always exits
/// 0 (even on error), so any failure in this path never blocks the user's
/// prompt submission. Errors go to stderr; stdout is left empty on failure.
fn cowork_drain_command(
    target: String,
    cwd: Option<PathBuf>,
    cwd_source: Option<String>,
    format: String,
    hook_runtime: Option<String>,
) -> Result<()> {
    use mempal::cowork::Tool;
    use mempal::cowork::inbox;
    use mempal::cowork::receipts::{self, DrainMeta};

    let inner: Result<(), Box<dyn std::error::Error>> = (|| {
        let target_tool = Tool::from_target_str(&target)
            .ok_or_else(|| format!("invalid target `{target}`: expected claude|codex"))?;
        let mempal_home = inbox::mempal_home();

        let resolved_cwd: PathBuf = match (cwd, cwd_source.as_deref()) {
            (Some(path), None) => path,
            (None, Some("stdin-json")) => {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                let payload: serde_json::Value = serde_json::from_str(&buf)?;
                let cwd_str = payload
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .ok_or("stdin JSON payload missing `cwd` string field")?;
                PathBuf::from(cwd_str)
            }
            (None, Some(other)) => {
                return Err(format!("unsupported --cwd-source: {other}").into());
            }
            (None, None) => return Err("must provide --cwd or --cwd-source".into()),
            (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        };

        let meta = DrainMeta {
            injected_as: format.clone(),
            hook_runtime,
            drained_at: mempal::cowork::peek::format_rfc3339(std::time::SystemTime::now()),
        };
        let messages =
            receipts::drain_with_receipt(&mempal_home, target_tool, &resolved_cwd, &meta)?;
        if messages.is_empty() {
            return Ok(());
        }
        let partner = target_tool
            .partner()
            .ok_or("target has no partner (auto)")?;
        let out = match format.as_str() {
            "plain" => inbox::format_plain(partner, &messages),
            "codex-hook-json" => inbox::format_codex_hook_json(partner, &messages)?,
            _ => return Err(format!("unknown format: {format}").into()),
        };
        print!("{out}");
        Ok(())
    })();

    if let Err(e) = inner {
        eprintln!("mempal cowork-drain: {e}");
    }
    Ok(())
}

/// `mempal cowork-status` — print current inbox state for both targets at
/// the given cwd. Read-only; does NOT drain.
fn cowork_status_command(cwd: PathBuf) -> Result<()> {
    use mempal::cowork::Tool;
    use mempal::cowork::inbox;

    let mempal_home = inbox::mempal_home();
    println!("Project: {}", cwd.display());
    println!();
    for target in [Tool::Claude, Tool::Codex] {
        let path = match inbox::inbox_path(&mempal_home, target, &cwd) {
            Ok(p) => p,
            Err(_) => {
                println!("{} inbox:  <invalid cwd>", target.dir_name());
                continue;
            }
        };
        if !path.exists() {
            println!("{} inbox:  0 messages", target.dir_name());
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let count = content.lines().filter(|l| !l.trim().is_empty()).count();
        let bytes = content.len();
        println!(
            "{} inbox:  {} message{}, {} B",
            target.dir_name(),
            count,
            if count == 1 { "" } else { "s" },
            bytes
        );
        for line in content.lines().take(3) {
            if let Ok(msg) = serde_json::from_str::<inbox::InboxMessage>(line) {
                println!("  from {} @ {}: {}", msg.from, msg.pushed_at, msg.content);
            }
        }
    }

    // P116 receipts summary — derived, read-only.
    if let Ok(states) = mempal::cowork::receipts::message_states(&mempal_home, &cwd) {
        let count_of = |status: &str| states.iter().filter(|s| s.status == status).count();
        println!();
        println!(
            "receipts: {} pending, {} drained, {} lost (see `mempal cowork-receipts --cwd {}`)",
            count_of("pending"),
            count_of("drained"),
            count_of("lost"),
            cwd.display()
        );
    }
    Ok(())
}

/// `mempal cowork-receipts` — P116 read-only delivery receipt view for the
/// pair-push channel: joins queued/drained events with the live inboxes.
fn cowork_receipts_command(cwd: PathBuf, limit: Option<usize>, format: String) -> Result<()> {
    use mempal::cowork::inbox;
    use mempal::cowork::receipts;

    let mempal_home = inbox::mempal_home();
    let mut states = receipts::message_states(&mempal_home, &cwd)
        .map_err(|e| anyhow::anyhow!("cowork-receipts: {e}"))?;
    if let Some(limit) = limit {
        states.truncate(limit);
    }

    match format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&states)?);
        }
        "plain" => {
            if states.is_empty() {
                println!("no delivery receipts for {}", cwd.display());
                return Ok(());
            }
            let count_of = |status: &str| states.iter().filter(|s| s.status == status).count();
            println!(
                "{} message{} ({} pending, {} drained, {} lost)",
                states.len(),
                if states.len() == 1 { "" } else { "s" },
                count_of("pending"),
                count_of("drained"),
                count_of("lost"),
            );
            for state in &states {
                let id = state.message_id.as_deref().unwrap_or("<pre-P116>");
                let queued = state.queued_at.as_deref().unwrap_or("-");
                let mut line = format!(
                    "{:8} {} {} -> {} queued={}",
                    state.status, id, state.from, state.to, queued
                );
                if let Some(drained_at) = &state.drained_at {
                    line.push_str(&format!(" drained={drained_at}"));
                }
                if let Some(injected_as) = &state.injected_as {
                    line.push_str(&format!(" injected_as={injected_as}"));
                }
                if let Some(hook_runtime) = &state.hook_runtime {
                    line.push_str(&format!(" hook_runtime={hook_runtime:?}"));
                }
                println!("{line}");
            }
        }
        other => anyhow::bail!("unsupported --format value: {other}"),
    }
    Ok(())
}

/// `mempal cowork-install-hooks` — install Claude Code project-level hook
/// script and optionally merge Codex global hooks.json entry.
fn cowork_install_hooks_command(global_codex: bool) -> Result<()> {
    let inner: Result<(), Box<dyn std::error::Error>> = (|| {
        // Claude Code hook (project-local) — TWO artifacts are needed:
        //   1. `.claude/hooks/user-prompt-submit.sh`  (the drain script)
        //   2. `.claude/settings.json` hooks.UserPromptSubmit entry
        //      registering that script with Claude Code's hook system.
        //
        // Claude Code does NOT auto-discover shell files by filename; a hook
        // must be declared in settings.json with type=command + command=path.
        // Dropping only the script file silently leaves the hook dead —
        // that was the P8 install-hooks ship bug surfaced by the first real
        // E2E run. This install now handles both artifacts with the same
        // self-heal classification used on the Codex side.
        let cwd = std::env::current_dir()?;
        let claude_dir = cwd.join(".claude/hooks");
        std::fs::create_dir_all(&claude_dir)?;
        let claude_script = claude_dir.join("user-prompt-submit.sh");
        let claude_content = r#"#!/bin/bash
# mempal cowork inbox drain — prepends partner handoff messages to user prompt
# Graceful degrade: any failure exits 0 with empty stdout
mempal cowork-drain --target claude --cwd "${CLAUDE_PROJECT_CWD:-$PWD}" 2>/dev/null || true
"#;
        std::fs::write(&claude_script, claude_content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&claude_script)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&claude_script, perms)?;
        }
        println!(
            "✓ installed Claude Code hook at {}",
            claude_script.display()
        );

        // Merge the hook registration into .claude/settings.json.
        const CANONICAL_CLAUDE_CMD: &str = "bash .claude/hooks/user-prompt-submit.sh";
        let settings_path = cwd.join(".claude/settings.json");
        let mut settings: serde_json::Value = if settings_path.exists() {
            let s = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&s).map_err(|e| {
                format!(
                    "refusing to overwrite existing .claude/settings.json — \
                     file is not valid JSON: {e}. Fix the file by hand and re-run."
                )
            })?
        } else {
            serde_json::json!({ "hooks": {} })
        };
        if !settings.is_object() {
            return Err(
                "refusing to overwrite .claude/settings.json — top-level value is not an object"
                    .into(),
            );
        }
        let hooks_field = settings
            .as_object_mut()
            .ok_or("settings.json root is not object")?
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));
        if !hooks_field.is_object() {
            return Err("`hooks` field in .claude/settings.json is not an object".into());
        }
        let hooks_obj = hooks_field
            .as_object_mut()
            .ok_or("hooks field is not object")?;
        let event_arr = hooks_obj
            .entry("UserPromptSubmit")
            .or_insert_with(|| serde_json::json!([]));
        let event_arr = event_arr
            .as_array_mut()
            .ok_or("UserPromptSubmit in .claude/settings.json is not array")?;

        let entry_has_drain_command = |entry: &serde_json::Value| -> Option<bool> {
            let hooks = entry.get("hooks")?.as_array()?;
            for handler in hooks {
                let cmd = handler.get("command")?.as_str()?;
                if cmd == CANONICAL_CLAUDE_CMD {
                    return Some(true);
                }
                // Treat any UserPromptSubmit entry pointing at our script
                // path OR invoking `mempal cowork-drain` directly as a
                // stale/older-version install that must be healed.
                if cmd.contains("user-prompt-submit.sh") || cmd.contains("mempal cowork-drain") {
                    return Some(false);
                }
            }
            None
        };

        let mut canonical_count = 0usize;
        let mut has_stale = false;
        for entry in event_arr.iter() {
            match entry_has_drain_command(entry) {
                Some(true) => canonical_count += 1,
                Some(false) => has_stale = true,
                None => {}
            }
        }

        let needs_rewrite = has_stale || canonical_count != 1;
        if !needs_rewrite {
            println!(
                "= Claude Code hook already registered in {} (no-op)",
                settings_path.display()
            );
        } else {
            event_arr.retain(|entry| entry_has_drain_command(entry).is_none());
            event_arr.push(serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": CANONICAL_CLAUDE_CMD,
                }]
            }));
            std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
            if has_stale {
                println!(
                    "✓ healed stale Claude Code drain hook in {}",
                    settings_path.display()
                );
            } else {
                println!(
                    "✓ registered Claude Code hook in {}",
                    settings_path.display()
                );
            }
        }

        if global_codex {
            // Do NOT introduce `dirs` crate — use env::var_os("HOME") directly.
            let home = match std::env::var_os("HOME") {
                Some(h) => PathBuf::from(h),
                None => return Err("cannot resolve $HOME env var".into()),
            };
            let codex_dir = home.join(".codex");
            std::fs::create_dir_all(&codex_dir)?;
            let hooks_path = codex_dir.join("hooks.json");

            let mut root: serde_json::Value = if hooks_path.exists() {
                let s = std::fs::read_to_string(&hooks_path)?;
                serde_json::from_str(&s)?
            } else {
                serde_json::json!({ "hooks": {} })
            };
            if !root.is_object() {
                root = serde_json::json!({ "hooks": {} });
            }
            let hooks_field = root
                .as_object_mut()
                .ok_or("hooks.json root is not object")?
                .entry("hooks")
                .or_insert_with(|| serde_json::json!({}));
            let hooks_obj = hooks_field
                .as_object_mut()
                .ok_or("hooks field is not object")?;
            let event_arr = hooks_obj
                .entry("UserPromptSubmit")
                .or_insert_with(|| serde_json::json!([]));
            let event_arr = event_arr
                .as_array_mut()
                .ok_or("UserPromptSubmit is not array")?;

            // Exact-match idempotency + self-healing: spec line 48 pins the
            // canonical command. Scan for any entry whose nested hooks
            // contain a `mempal cowork-drain` command. Classify each match as
            // either (a) exact-match of CANONICAL, or (b) stale/wrong drain
            // entry that must be replaced. Unrelated entries (non-drain
            // commands) are preserved untouched.
            //
            // Outcomes:
            //  - exactly one canonical entry AND no stale entries → no-op
            //  - any stale entry present OR canonical missing → remove every
            //    mempal-drain entry and re-append canonical
            //
            // This way a user re-running install-hooks after upgrading mempal
            // (where the command flags changed) gets their stale hook healed
            // instead of silently left broken by a loose substring match.
            const CANONICAL_CODEX_CMD: &str = "mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json";

            let entry_has_drain_command = |entry: &serde_json::Value| -> Option<bool> {
                // Returns Some(true) for exact canonical, Some(false) for
                // stale drain, None for unrelated.
                let hooks = entry.get("hooks")?.as_array()?;
                for handler in hooks {
                    let cmd = handler.get("command")?.as_str()?;
                    if cmd == CANONICAL_CODEX_CMD {
                        return Some(true);
                    }
                    if cmd.contains("mempal cowork-drain") {
                        return Some(false);
                    }
                }
                None
            };

            let mut canonical_count = 0usize;
            let mut has_stale = false;
            for entry in event_arr.iter() {
                match entry_has_drain_command(entry) {
                    Some(true) => canonical_count += 1,
                    Some(false) => has_stale = true,
                    None => {}
                }
            }

            let needs_rewrite = has_stale || canonical_count != 1;

            if !needs_rewrite {
                println!(
                    "= Codex hook already installed in {} (no-op)",
                    hooks_path.display()
                );
            } else {
                event_arr.retain(|entry| entry_has_drain_command(entry).is_none());
                event_arr.push(serde_json::json!({
                    "hooks": [{
                        "type": "command",
                        "command": CANONICAL_CODEX_CMD,
                        "statusMessage": "mempal cowork drain"
                    }]
                }));

                std::fs::write(&hooks_path, serde_json::to_string_pretty(&root)?)?;
                if has_stale {
                    println!(
                        "✓ healed stale Codex drain hook in {}",
                        hooks_path.display()
                    );
                } else {
                    println!("✓ merged Codex hook into {}", hooks_path.display());
                }
            }

            // Feature flag gate: Codex's hooks runtime is behind the
            // `codex_hooks` feature flag, which is "under development" and
            // OFF by default in shipped `codex-cli` (<= 0.120.0 at time of
            // writing). When the flag is false, Codex silently ignores
            // ~/.codex/hooks.json regardless of shape — the install above
            // will appear to succeed but the hook will never fire. Surface
            // this to the user so they can opt in explicitly with
            // `codex features enable codex_hooks`.
            if !codex_hooks_feature_enabled(&codex_dir) {
                println!();
                println!("⚠  Codex `codex_hooks` feature is currently disabled.");
                println!("   This is an 'under development' feature in shipped Codex and is OFF");
                println!("   by default. Without it, ~/.codex/hooks.json is silently ignored and");
                println!("   the hook you just installed will never fire on user prompt submit.");
                println!();
                println!("   To activate:");
                println!("     codex features enable codex_hooks");
                println!();
                println!("   Or equivalent: add `codex_hooks = true` under `[features]` in");
                println!("     ~/.codex/config.toml");
            }
        }

        println!();
        println!("Next steps:");
        println!(
            "  1. Claude Code picks up settings.json changes on the next prompt — no restart needed"
        );
        println!(
            "  2. Restart Codex TUI so it re-reads ~/.codex/hooks.json (session-scoped cache)"
        );
        println!("  3. Test: ask Claude to push a test message to codex;");
        println!("     then in Codex, type anything — the message should be prepended");

        Ok(())
    })();

    if let Err(e) = inner {
        eprintln!("mempal cowork-install-hooks: {e}");
        return Err(anyhow::anyhow!("cowork-install-hooks failed"));
    }
    Ok(())
}

/// Check whether the Codex `codex_hooks` feature flag is enabled in
/// `<codex_dir>/config.toml`. Returns true only if the file contains a
/// key `codex_hooks` (either as a bare key inside `[features]` or as a
/// dotted top-level key `features.codex_hooks`) whose value is the literal
/// `true`. Any other state — missing file, missing key, `false`, or
/// unparseable — returns false and triggers the "install succeeded but
/// Codex runtime will ignore it" warning in install-hooks.
///
/// This is a deliberate minimal string-scan parser. We do not pull in the
/// `toml` crate because (a) the spec forbids new runtime dependencies and
/// (b) a false warning is cheap while a false all-clear would hide the
/// very bug this check exists to surface.
fn codex_hooks_feature_enabled(codex_dir: &Path) -> bool {
    let config_path = codex_dir.join("config.toml");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return false;
    };
    for line in content.lines() {
        // Drop any inline `#` comment tail. TOML doesn't allow `#` inside
        // unquoted strings on the RHS of a key=value line, so this is safe
        // for our narrow `codex_hooks = true` match.
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let bare_key = key.strip_prefix("features.").unwrap_or(key);
        if bare_key == "codex_hooks" && val.trim() == "true" {
            return true;
        }
    }
    false
}

fn parse_keywords_arg(keywords: &str) -> Vec<String> {
    keywords
        .split(',')
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn render_room(room: Option<&str>) -> &str {
    match room {
        Some(room) if !room.is_empty() => room,
        _ => "default",
    }
}

fn truncate_for_summary(content: &str, limit: usize) -> String {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        return compact;
    }

    compact.chars().take(limit).collect::<String>() + "..."
}

fn estimate_wake_up_tokens(drawers: &[mempal::core::types::Drawer]) -> usize {
    drawers
        .iter()
        .map(|drawer| effective_wake_up_text(drawer).split_whitespace().count())
        .sum()
}

fn detect_rooms_with_options(
    dir: &Path,
    filter_options: &ProjectPathFilterOptions,
) -> Result<Vec<String>> {
    let mut rooms = BTreeSet::new();
    for entry in project_walk(dir, filter_options)? {
        let entry = entry.with_context(|| format!("failed to walk {}", dir.display()))?;
        let path = entry.path();
        if path == dir || !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && !matches!(name, "src" | "tests")
        {
            rooms.insert(name.to_string());
        }
    }

    Ok(rooms.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use mempal::path_filter::ProjectPathFilterOptions;

    fn run_cli_parse_test(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .name("cli-parse-test".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(test)
            .expect("spawn cli parse test")
            .join()
            .expect("cli parse test");
    }

    #[test]
    fn test_init_detect_rooms_respects_gitignore_and_mempalignore() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src/auth")).expect("src auth");
        std::fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        std::fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        std::fs::create_dir_all(root.join("target/release-room")).expect("target");
        std::fs::create_dir_all(root.join("node_modules/pkg-room")).expect("node_modules");
        std::fs::write(root.join(".gitignore"), "dist/\n").expect("gitignore");
        std::fs::write(root.join(".mempalignore"), "generated/\n").expect("mempalignore");

        let rooms = detect_rooms_with_options(root, &ProjectPathFilterOptions::default())
            .expect("detect rooms");

        assert!(rooms.contains(&"auth".to_string()));
        assert!(!rooms.contains(&"build-room".to_string()));
        assert!(!rooms.contains(&"gen-room".to_string()));
        assert!(!rooms.contains(&"release-room".to_string()));
        assert!(!rooms.contains(&"pkg-room".to_string()));
    }

    #[test]
    fn test_init_no_gitignore_keeps_mempalignore() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        std::fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        std::fs::write(root.join(".gitignore"), "dist/\n").expect("gitignore");
        std::fs::write(root.join(".mempalignore"), "generated/\n").expect("mempalignore");

        let rooms = detect_rooms_with_options(
            root,
            &ProjectPathFilterOptions {
                respect_gitignore: false,
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("detect rooms");

        assert!(rooms.contains(&"build-room".to_string()));
        assert!(!rooms.contains(&"gen-room".to_string()));
    }

    #[test]
    fn test_init_custom_ignore_file_excludes_rooms() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        let custom = root.join("custom.ignore");
        std::fs::create_dir_all(root.join("notes/api")).expect("notes api");
        std::fs::write(&custom, "notes/\n").expect("custom ignore");

        let rooms = detect_rooms_with_options(
            root,
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![custom],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("detect rooms");

        assert!(!rooms.contains(&"api".to_string()));
    }

    #[test]
    fn test_init_missing_ignore_file_fails() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let missing = tmp.path().join("missing.ignore");
        let err = detect_rooms_with_options(
            tmp.path(),
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![missing.clone()],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect_err("missing custom ignore file should fail");

        assert!(err.to_string().contains(&missing.display().to_string()));
    }

    #[test]
    fn test_cli_init_ignore_flags_parse() {
        run_cli_parse_test(|| {
            let matches = Cli::command()
                .try_get_matches_from([
                    "mempal",
                    "init",
                    ".",
                    "--dry-run",
                    "--ignore-file",
                    "extra.ignore",
                    "--no-gitignore",
                    "--no-mempalignore",
                ])
                .expect("init ignore flags must parse");
            let (name, sub) = matches.subcommand().expect("subcommand");
            assert_eq!(name, "init");
            assert_eq!(sub.get_one::<PathBuf>("dir"), Some(&PathBuf::from(".")));
            assert!(sub.get_flag("dry_run"));
            let ignore_files = sub
                .get_many::<PathBuf>("ignore_files")
                .expect("ignore files")
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(ignore_files, vec![PathBuf::from("extra.ignore")]);
            assert!(sub.get_flag("no_gitignore"));
            assert!(sub.get_flag("no_mempalignore"));
        });
    }

    #[test]
    fn test_cli_ingest_ignore_flags_parse() {
        run_cli_parse_test(|| {
            let matches = Cli::command()
                .try_get_matches_from([
                    "mempal",
                    "ingest",
                    ".",
                    "--wing",
                    "demo",
                    "--ignore-file",
                    "extra.ignore",
                    "--no-gitignore",
                    "--no-mempalignore",
                ])
                .expect("ingest ignore flags must parse");
            let (name, sub) = matches.subcommand().expect("subcommand");
            assert_eq!(name, "ingest");
            assert_eq!(sub.get_one::<PathBuf>("dir"), Some(&PathBuf::from(".")));
            assert_eq!(
                sub.get_one::<String>("wing").map(String::as_str),
                Some("demo")
            );
            assert!(sub.get_one::<String>("room").is_none());
            assert!(sub.get_one::<String>("format").is_none());
            assert!(!sub.get_flag("dry_run"));
            assert!(!sub.get_flag("no_strip_noise"));
            assert!(!sub.get_flag("diary_rollup"));
            let ignore_files = sub
                .get_many::<PathBuf>("ignore_files")
                .expect("ignore files")
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(ignore_files, vec![PathBuf::from("extra.ignore")]);
            assert!(sub.get_flag("no_gitignore"));
            assert!(sub.get_flag("no_mempalignore"));
        });
    }

    // P108: the cross-project cowork-peek CLI must parse with an explicit tool
    // and a target project cwd.
    #[test]
    fn test_cli_cowork_peek_parses() {
        run_cli_parse_test(|| {
            let matches = Cli::command()
                .try_get_matches_from([
                    "mempal",
                    "cowork-peek",
                    "--tool",
                    "codex",
                    "--cwd",
                    "/tmp/project",
                ])
                .expect("cowork-peek must parse");
            let (name, sub) = matches.subcommand().expect("subcommand");
            assert_eq!(name, "cowork-peek");
            assert_eq!(
                sub.get_one::<String>("tool").map(String::as_str),
                Some("codex")
            );
            assert_eq!(
                sub.get_one::<PathBuf>("cwd"),
                Some(&PathBuf::from("/tmp/project"))
            );
            assert_eq!(sub.get_one::<usize>("limit"), Some(&30), "default limit");
            assert!(sub.get_one::<String>("since").is_none());
            assert_eq!(
                sub.get_one::<String>("format").map(String::as_str),
                Some("plain"),
                "default format"
            );
        });
    }

    // P109: projects + resume CLI must parse.
    #[test]
    fn test_cli_projects_and_resume_parse() {
        run_cli_parse_test(|| {
            let projects = Cli::command()
                .try_get_matches_from(["mempal", "projects", "--format", "json"])
                .expect("projects must parse");
            let (name, sub) = projects.subcommand().expect("projects subcommand");
            assert_eq!(name, "projects");
            assert_eq!(
                sub.get_one::<String>("format").map(String::as_str),
                Some("json")
            );

            let resume = Cli::command()
                .try_get_matches_from(["mempal", "resume", "auth-service"])
                .expect("resume must parse");
            let (name, sub) = resume.subcommand().expect("resume subcommand");
            assert_eq!(name, "resume");
            assert_eq!(
                sub.get_one::<String>("query").map(String::as_str),
                Some("auth-service")
            );
            assert_eq!(
                sub.get_one::<usize>("evidence_limit"),
                Some(&5),
                "default evidence limit"
            );
            assert_eq!(
                sub.get_one::<usize>("candidate_limit"),
                Some(&5),
                "default candidate limit"
            );
            assert_eq!(
                sub.get_one::<String>("format").map(String::as_str),
                Some("plain"),
                "default format"
            );
        });
    }

    #[test]
    fn test_expand_home_handles_tilde_without_home_env() {
        let home = PathBuf::from("/tmp/mempal-home");

        assert_eq!(expand_home_with_home("~", Some(home.clone())), home);
        assert_eq!(
            expand_home_with_home("~/palace.db", Some(home.clone())),
            home.join("palace.db")
        );

        let fallback = expand_home_with_home("~/palace.db", None);
        assert_ne!(fallback, PathBuf::from("~/palace.db"));
        assert_eq!(fallback, PathBuf::from(".").join("palace.db"));
    }
}
