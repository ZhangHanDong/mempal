use std::collections::BTreeSet;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(feature = "rest")]
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mempal_aaak::{AaakCodec, AaakMeta};
#[cfg(feature = "rest")]
use mempal_api::{ApiState, DEFAULT_REST_ADDR, serve as serve_rest_api};
use mempal_core::{
    config::Config,
    db::Database,
    protocol::{DEFAULT_IDENTITY_HINT, MEMORY_PROTOCOL},
    types::TaxonomyEntry,
    utils::current_timestamp,
};
use mempal_embed::{ConfiguredEmbedderFactory, Embedder};
use mempal_ingest::{IngestOptions, IngestStats, ingest_dir, ingest_dir_with_options};
use mempal_mcp::MempalMcpServer;
use mempal_search::search;

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
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    Serve {
        #[arg(long)]
        mcp: bool,
    },
    Status,
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
        Commands::Taxonomy { command } => taxonomy_command(&db, command),
        Commands::Serve { mcp } => serve_command(&config, mcp).await,
        Commands::Status => status_command(&db),
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
    ) -> std::result::Result<Vec<Vec<f32>>, mempal_embed::EmbedError> {
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
    let recent_drawers = db
        .recent_drawers(5)
        .context("failed to load recent drawers for wake-up")?;
    let token_estimate = estimate_tokens(&recent_drawers);

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
    if recent_drawers.is_empty() {
        println!("no recent drawers");
    } else {
        for drawer in &recent_drawers {
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
    let recent_drawers = db
        .recent_drawers(5)
        .context("failed to load recent drawers for AAAK wake-up")?;
    let text = if recent_drawers.is_empty() {
        "mempal wake-up: no recent drawers".to_string()
    } else {
        recent_drawers
            .iter()
            .map(|drawer| drawer.content.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let wing = recent_drawers
        .first()
        .map(|drawer| drawer.wing.as_str())
        .unwrap_or("mempal");
    let room = recent_drawers
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
    println!("taxonomy_entries: {taxonomy_count}");
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
    use mempal_embed::EmbedderFactory;
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

fn estimate_tokens(drawers: &[mempal_core::types::Drawer]) -> usize {
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
