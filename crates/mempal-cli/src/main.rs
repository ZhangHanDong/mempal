use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mempal_core::{
    config::Config,
    db::Database,
    types::TaxonomyEntry,
};
use mempal_embed::{Embedder, api::ApiEmbedder, onnx::OnnxEmbedder};
use mempal_ingest::ingest_dir;
use mempal_search::search;

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
    },
    Ingest {
        dir: PathBuf,
        #[arg(long)]
        wing: String,
        #[arg(long)]
        format: Option<String>,
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
    WakeUp,
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
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
        Commands::Init { dir } => init_command(&db, &dir),
        Commands::Ingest { dir, wing, format } => {
            ingest_command(&db, &config, &dir, &wing, format).await
        }
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
        Commands::WakeUp => wake_up_command(&db),
        Commands::Taxonomy { command } => taxonomy_command(&db, command),
        Commands::Status => status_command(&db),
    }
}

fn init_command(db: &Database, dir: &Path) -> Result<()> {
    let wing = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
        .to_string();
    let rooms = detect_rooms(dir)?;

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
) -> Result<()> {
    if let Some(format) = format.as_deref()
        && format != "convos"
    {
        bail!("unsupported --format value: {format}");
    }

    let embedder = build_embedder(config).await?;
    let stats = ingest_dir(db, &*embedder, dir, wing, None).await?;

    println!(
        "files={} chunks={} skipped={}",
        stats.files, stats.chunks, stats.skipped
    );

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
        let source_file = result
            .source_file
            .unwrap_or_else(|| "<unknown>".to_string());
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

fn wake_up_command(db: &Database) -> Result<()> {
    let drawer_count = query_count(db, "drawers")?;
    let taxonomy_count = query_count(db, "taxonomy")?;
    let recent_drawers = db
        .recent_drawers(5)
        .context("failed to load recent drawers for wake-up")?;
    let token_estimate = estimate_tokens(&recent_drawers);

    println!("L0");
    println!("identity: mempal project memory for coding agents");
    println!("drawer_count: {drawer_count}");
    println!("taxonomy_entries: {taxonomy_count}");
    println!();
    println!("L1");
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
    let drawer_count = query_count(db, "drawers")?;
    let taxonomy_count = query_count(db, "taxonomy")?;
    let db_size_bytes = db
        .database_size_bytes()
        .context("failed to compute database size")?;

    println!("drawer_count: {drawer_count}");
    println!("taxonomy_entries: {taxonomy_count}");
    println!("db_size_bytes: {db_size_bytes}");

    let mut statement = db
        .conn()
        .prepare(
            r#"
            SELECT wing, room, COUNT(*)
            FROM drawers
            GROUP BY wing, room
            ORDER BY wing, room
            "#,
        )
        .context("failed to prepare status query")?;
    let counts = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .context("failed to execute status query")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect status rows")?;

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

async fn build_embedder(config: &Config) -> Result<Box<dyn Embedder>> {
    match config.embed.backend.as_str() {
        "onnx" => Ok(Box::new(
            OnnxEmbedder::new_or_download()
                .await
                .context("failed to initialize ONNX embedder")?,
        )),
        "api" => Ok(Box::new(ApiEmbedder::new(
            config
                .embed
                .api_endpoint
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/api/embeddings".to_string()),
            config.embed.api_model.clone(),
            384,
        ))),
        backend => bail!("unsupported embed backend: {backend}"),
    }
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }

    PathBuf::from(path)
}

fn query_count(db: &Database, table: &str) -> Result<i64> {
    db.conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .with_context(|| format!("failed to count rows in {table}"))
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
