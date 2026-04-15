use std::collections::BTreeSet;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(feature = "rest")]
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mempal::aaak::{AaakCodec, AaakMeta};
#[cfg(feature = "rest")]
use mempal::api::{ApiState, DEFAULT_REST_ADDR, serve as serve_rest_api};
use mempal::core::{
    config::Config,
    db::Database,
    protocol::{DEFAULT_IDENTITY_HINT, MEMORY_PROTOCOL},
    types::TaxonomyEntry,
    utils::{build_triple_id, current_timestamp},
};
use mempal::embed::{ConfiguredEmbedderFactory, Embedder};
use mempal::ingest::{IngestOptions, IngestStats, ingest_dir, ingest_dir_with_options};
use mempal::mcp::MempalMcpServer;
use mempal::search::search;

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
    Init {
        dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Ingest {
        dir: PathBuf,
        #[arg(long)]
        wing: String,
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Search {
        query: String,
        #[arg(long)]
        wing: Option<String>,
        #[arg(long)]
        room: Option<String>,
        #[arg(long, default_value_t = 10)]
        top_k: usize,
        #[arg(long)]
        json: bool,
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
    Reindex,
    Kg {
        #[command(subcommand)]
        command: KgCommands,
    },
    Tunnels,
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    Serve {
        #[arg(long)]
        mcp: bool,
    },
    Status,
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
    },
    /// Show current cowork inbox state for both targets at the given cwd
    /// (read-only — does NOT drain).
    CoworkStatus {
        #[arg(long)]
        cwd: PathBuf,
    },
    /// Install cowork hooks: Claude Code (project-level .claude/hooks)
    /// and optionally Codex (global ~/.codex/hooks.json merge).
    CoworkInstallHooks {
        #[arg(long, default_value_t = false)]
        global_codex: bool,
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
        Commands::CoworkDrain {
            target,
            cwd,
            cwd_source,
            format,
        } => {
            return cowork_drain_command(target, cwd, cwd_source, format);
        }
        Commands::CoworkStatus { cwd } => {
            return cowork_status_command(cwd);
        }
        Commands::CoworkInstallHooks { global_codex } => {
            return cowork_install_hooks_command(global_codex);
        }
        // All other commands fall through to the db-backed dispatch below.
        _ => {}
    }

    let config = Config::load().context("failed to load config")?;
    let db = Database::open(&expand_home(&config.db_path)).context("failed to open database")?;

    match cli.command {
        Commands::Init { dir, dry_run } => init_command(&db, &dir, dry_run),
        Commands::Ingest {
            dir,
            wing,
            format,
            dry_run,
        } => ingest_command(&db, &config, &dir, &wing, format, dry_run).await,
        Commands::Search {
            query,
            wing,
            room,
            top_k,
            json,
        } => {
            search_command(
                &db,
                &config,
                &query,
                wing.as_deref(),
                room.as_deref(),
                top_k,
                json,
            )
            .await
        }
        Commands::Delete { drawer_id } => delete_command(&db, &drawer_id),
        Commands::Purge { before } => purge_command(&db, before.as_deref()),
        Commands::WakeUp { format } => wake_up_command(&db, format.as_deref()),
        Commands::Compress { text } => compress_command(&text),
        Commands::Bench { command } => bench_command(&config, command).await,
        Commands::Reindex => reindex_command(&db, &config).await,
        Commands::Kg { command } => kg_command(&db, command),
        Commands::Tunnels => tunnels_command(&db),
        Commands::Taxonomy { command } => taxonomy_command(&db, command),
        Commands::Serve { mcp } => serve_command(&config, mcp).await,
        Commands::Status => status_command(&db),
        // Cowork commands were already dispatched above and returned early.
        Commands::CoworkDrain { .. }
        | Commands::CoworkStatus { .. }
        | Commands::CoworkInstallHooks { .. } => unreachable!(),
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

fn init_command(db: &Database, dir: &Path, dry_run: bool) -> Result<()> {
    let wing = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
        .to_string();
    let rooms = detect_rooms(dir)?;

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

async fn ingest_command(
    db: &Database,
    config: &Config,
    dir: &Path,
    wing: &str,
    format: Option<String>,
    dry_run: bool,
) -> Result<()> {
    if let Some(format) = format.as_deref()
        && format != "convos"
    {
        bail!("unsupported --format value: {format}");
    }

    let stats = if dry_run {
        ingest_dir_with_options(
            db,
            &NoopEmbedder,
            dir,
            wing,
            IngestOptions {
                room: None,
                source_root: Some(dir),
                dry_run: true,
            },
        )
        .await?
    } else {
        let embedder = build_embedder(config).await?;
        ingest_dir(db, &*embedder, dir, wing, None).await?
    };

    append_ingest_audit_log(db, dir, wing, format.as_deref(), dry_run, stats)
        .context("failed to append ingest audit log")?;

    println!(
        "dry_run={} files={} chunks={} skipped={}",
        dry_run, stats.files, stats.chunks, stats.skipped
    );

    Ok(())
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

async fn search_command(
    db: &Database,
    config: &Config,
    query: &str,
    wing: Option<&str>,
    room: Option<&str>,
    top_k: usize,
    json: bool,
) -> Result<()> {
    let embedder = build_embedder(config).await?;
    let results = search(db, &*embedder, query, wing, room, top_k).await?;

    if json {
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
        let room = result.room.unwrap_or_else(|| "default".to_string());
        let source_file = result.source_file;
        println!(
            "[{:.3}] {}/{} {}",
            result.similarity, result.wing, room, result.drawer_id
        );
        println!("source: {source_file}");
        if !result.tunnel_hints.is_empty() {
            println!("tunnel: also in {}", result.tunnel_hints.join(", "));
        }
        println!("{}", result.content);
        println!();
    }

    Ok(())
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
    let token_estimate = estimate_tokens(&top_drawers);

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
            println!("  {}", truncate_for_summary(&drawer.content, 120));
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
            .map(|drawer| drawer.content.as_str())
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

async fn reindex_command(db: &Database, config: &Config) -> Result<()> {
    let embedder = build_embedder(config).await?;
    let new_dim = embedder.dimensions();
    let current_dim = db.embedding_dim().context("failed to read embedding dim")?;

    println!("embedder: {} ({}d)", embedder.name(), new_dim);
    if let Some(dim) = current_dim {
        println!("current vector dim: {dim}");
    } else {
        println!("current vector dim: (empty table)");
    }

    // Recreate vectors table with new dimension
    println!("recreating drawer_vectors with {new_dim} dimensions...");
    db.recreate_vectors_table(new_dim)
        .context("failed to recreate vectors table")?;

    // Re-embed all active drawers
    let drawers = db
        .all_active_drawers()
        .context("failed to load active drawers")?;
    let total = drawers.len();
    println!("re-embedding {total} drawers...");

    let batch_size = 64;
    let mut done = 0;
    for chunk in drawers.chunks(batch_size) {
        let texts: Vec<&str> = chunk.iter().map(|(_, content)| content.as_str()).collect();
        let vectors = embedder.embed(&texts).await.context("embedding failed")?;

        for ((id, _), vector) in chunk.iter().zip(vectors.iter()) {
            db.insert_vector(id, vector)
                .with_context(|| format!("failed to insert vector for {id}"))?;
        }

        done += chunk.len();
        if total > batch_size {
            println!("  {done}/{total}");
        }
    }

    println!("reindex complete: {total} drawers, {new_dim}d vectors");
    Ok(())
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

fn tunnels_command(db: &Database) -> Result<()> {
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

    println!("schema_version: {schema_version}");
    println!("drawer_count: {drawer_count}");
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
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }

    PathBuf::from(path)
}

/// `mempal cowork-drain` — called by UserPromptSubmit hooks. Always exits
/// 0 (even on error), so any failure in this path never blocks the user's
/// prompt submission. Errors go to stderr; stdout is left empty on failure.
fn cowork_drain_command(
    target: String,
    cwd: Option<PathBuf>,
    cwd_source: Option<String>,
    format: String,
) -> Result<()> {
    use mempal::cowork::Tool;
    use mempal::cowork::inbox;

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

        let messages = inbox::drain(&mempal_home, target_tool, &resolved_cwd)?;
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

fn estimate_tokens(drawers: &[mempal::core::types::Drawer]) -> usize {
    drawers
        .iter()
        .map(|drawer| drawer.content.split_whitespace().count())
        .sum()
}

fn detect_rooms(dir: &Path) -> Result<Vec<String>> {
    let mut rooms = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)
            .with_context(|| format!("failed to read directory {}", current.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", current.display()))?;
            let path = entry.path();
            if !path.is_dir() || should_skip_dir(&path) {
                continue;
            }

            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && !matches!(name, "src" | "tests")
            {
                rooms.insert(name.to_string());
            }

            stack.push(path);
        }
    }

    Ok(rooms.into_iter().collect())
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| matches!(name, ".git" | "target" | "node_modules"))
        .unwrap_or(false)
}
