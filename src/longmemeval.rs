use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use mempal::aaak::{AaakCodec, AaakMeta};
use mempal::core::{
    config::Config,
    db::Database,
    types::{Drawer, SourceType, TaxonomyEntry},
    utils::{build_drawer_id, route_room_from_taxonomy},
};
use mempal::embed::{ConfiguredEmbedderFactory, Embedder};
use mempal::search::search;
use serde::{Deserialize, Deserializer, Serialize};
use tempfile::tempdir;

const BENCH_WING: &str = "longmemeval";
const DEFAULT_TOP_K: usize = 50;
const METRIC_KS: [usize; 6] = [1, 3, 5, 10, 30, 50];
const TECHNICAL_KEYWORDS: &[&str] = &[
    "code", "python", "function", "bug", "error", "api", "database", "server", "deploy", "git",
    "test", "debug", "refactor",
];
const PLANNING_KEYWORDS: &[&str] = &[
    "plan",
    "roadmap",
    "milestone",
    "deadline",
    "priority",
    "sprint",
    "backlog",
    "scope",
    "requirement",
    "spec",
];
const DECISION_KEYWORDS: &[&str] = &[
    "decided",
    "chose",
    "picked",
    "switched",
    "migrated",
    "replaced",
    "trade-off",
    "alternative",
    "option",
    "approach",
];
const PERSONAL_KEYWORDS: &[&str] = &[
    "family", "friend", "birthday", "vacation", "hobby", "health", "feeling", "love", "home",
    "weekend",
];
const KNOWLEDGE_KEYWORDS: &[&str] = &[
    "learn",
    "study",
    "degree",
    "school",
    "university",
    "course",
    "research",
    "paper",
    "book",
    "reading",
];
const ROOM_KEYWORDS: &[(&str, &[&str])] = &[
    ("technical", TECHNICAL_KEYWORDS),
    ("planning", PLANNING_KEYWORDS),
    ("decisions", DECISION_KEYWORDS),
    ("personal", PERSONAL_KEYWORDS),
    ("knowledge", KNOWLEDGE_KEYWORDS),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BenchMode {
    Raw,
    Aaak,
    Rooms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LongMemEvalGranularity {
    Session,
    Turn,
}

#[derive(Debug, Clone)]
pub struct LongMemEvalArgs {
    pub data_file: PathBuf,
    pub mode: BenchMode,
    pub granularity: LongMemEvalGranularity,
    pub limit: usize,
    pub skip: usize,
    pub top_k: usize,
    pub out: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct LongMemEvalTurn {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LongMemEvalEntry {
    question_id: String,
    question_type: String,
    question: String,
    #[serde(deserialize_with = "deserialize_answer")]
    answer: String,
    haystack_sessions: Vec<Vec<LongMemEvalTurn>>,
    haystack_session_ids: Vec<String>,
    haystack_dates: Vec<String>,
    answer_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LongMemEvalAnswerValue {
    Text(String),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
}

fn deserialize_answer<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = LongMemEvalAnswerValue::deserialize(deserializer)?;
    Ok(match value {
        LongMemEvalAnswerValue::Text(text) => text,
        LongMemEvalAnswerValue::Signed(number) => number.to_string(),
        LongMemEvalAnswerValue::Unsigned(number) => number.to_string(),
        LongMemEvalAnswerValue::Float(number) => number.to_string(),
        LongMemEvalAnswerValue::Bool(value) => value.to_string(),
    })
}

#[derive(Debug, Clone)]
struct CorpusItem {
    corpus_id: String,
    original_text: String,
    retrieval_text: String,
    timestamp: String,
    drawer_id: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct AggregateMetrics {
    recall_any: BTreeMap<usize, Vec<f64>>,
    recall_all: BTreeMap<usize, Vec<f64>>,
    ndcg_any: BTreeMap<usize, Vec<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct MetricSnapshot {
    recall_any: BTreeMap<usize, f64>,
    ndcg_any: BTreeMap<usize, f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct EntryMetricSnapshot {
    session: MetricSnapshot,
    turn: MetricSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
struct BenchmarkSummary {
    mode: BenchMode,
    granularity: LongMemEvalGranularity,
    question_count: usize,
    elapsed_secs: f64,
    session: MetricSnapshot,
    turn: MetricSnapshot,
    per_type: BTreeMap<String, MetricSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct RankedItemLog {
    corpus_id: String,
    text: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct RetrievalLog {
    query: String,
    ranked_items: Vec<RankedItemLog>,
    metrics: RetrievalMetricLog,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct RetrievalMetricLog {
    session: BTreeMap<String, f64>,
    turn: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct BenchmarkLogEntry {
    question_id: String,
    question_type: String,
    question: String,
    answer: String,
    retrieval_results: RetrievalLog,
}

pub async fn run_longmemeval_command(config: &Config, args: LongMemEvalArgs) -> Result<()> {
    if args.top_k == 0 {
        bail!("--top-k must be greater than 0");
    }

    use mempal::embed::EmbedderFactory;

    let entries = load_entries(&args.data_file, args.limit, args.skip)?;
    let embedder = ConfiguredEmbedderFactory::new(config.clone())
        .build()
        .await
        .context("failed to initialize embedder for LongMemEval benchmark")?;
    let (summary, logs) = run_benchmark_with_embedder(&*embedder, &entries, &args).await?;

    print_summary(&summary);

    if let Some(out_path) = args.out.as_deref() {
        write_results_log(out_path, &logs)?;
        println!("results_log: {}", out_path.display());
    }

    Ok(())
}

fn load_entries(path: &Path, limit: usize, skip: usize) -> Result<Vec<LongMemEvalEntry>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read LongMemEval data from {}", path.display()))?;
    let mut entries = serde_json::from_str::<Vec<LongMemEvalEntry>>(&contents)
        .with_context(|| format!("failed to parse LongMemEval JSON {}", path.display()))?;

    if skip > 0 {
        entries = entries.into_iter().skip(skip).collect();
    }
    if limit > 0 {
        entries.truncate(limit);
    }

    Ok(entries)
}

async fn run_benchmark_with_embedder<E: Embedder + ?Sized>(
    embedder: &E,
    entries: &[LongMemEvalEntry],
    args: &LongMemEvalArgs,
) -> Result<(BenchmarkSummary, Vec<BenchmarkLogEntry>)> {
    let ks = selected_ks(args.top_k);
    let started = Instant::now();
    let scratch = tempdir().context("failed to create benchmark scratch directory")?;

    let mut session_metrics = AggregateMetrics::default();
    let mut turn_metrics = AggregateMetrics::default();
    let mut per_type = BTreeMap::<String, AggregateMetrics>::new();
    let mut logs = Vec::with_capacity(entries.len());

    for (index, entry) in entries.iter().enumerate() {
        let db_path = scratch.path().join(format!("q-{index}.db"));
        let db = Database::open(&db_path)
            .with_context(|| format!("failed to open benchmark database {}", db_path.display()))?;

        if args.mode == BenchMode::Rooms {
            install_rooms_taxonomy(&db)?;
        }

        let corpus_items = build_corpus(entry, args.granularity, args.mode);
        if corpus_items.is_empty() {
            continue;
        }

        ingest_corpus(&db, embedder, &corpus_items, args.mode).await?;

        let results = search(&db, embedder, &entry.question, None, None, args.top_k)
            .await
            .with_context(|| format!("search failed for question {}", entry.question_id))?;
        let rankings = map_results_to_rankings(&results, &corpus_items);
        let entry_metrics = score_entry(entry, &rankings, &corpus_items, &ks);

        for &k in &ks {
            push_metric_set(&mut session_metrics, k, &entry_metrics.session);
            push_metric_set(&mut turn_metrics, k, &entry_metrics.turn);
            let bucket = per_type.entry(entry.question_type.clone()).or_default();
            push_metric_set(bucket, k, &entry_metrics.session);
        }

        logs.push(build_log_entry(
            entry,
            &corpus_items,
            &rankings,
            &entry_metrics,
            args.top_k,
        ));
    }

    let elapsed_secs = started.elapsed().as_secs_f64();
    let summary = BenchmarkSummary {
        mode: args.mode,
        granularity: args.granularity,
        question_count: logs.len(),
        elapsed_secs,
        session: summarize_metrics(&session_metrics),
        turn: summarize_metrics(&turn_metrics),
        per_type: per_type
            .into_iter()
            .map(|(name, metrics)| (name, summarize_metrics(&metrics)))
            .collect(),
    };

    Ok((summary, logs))
}

fn build_corpus(
    entry: &LongMemEvalEntry,
    granularity: LongMemEvalGranularity,
    mode: BenchMode,
) -> Vec<CorpusItem> {
    let codec = AaakCodec::default();
    let mut items = Vec::new();

    for ((session, session_id), date) in entry
        .haystack_sessions
        .iter()
        .zip(entry.haystack_session_ids.iter())
        .zip(entry.haystack_dates.iter())
    {
        match granularity {
            LongMemEvalGranularity::Session => {
                let Some(text) = join_user_turns(session) else {
                    continue;
                };
                let item_ordinal = items.len();
                items.push(build_corpus_item(
                    &codec,
                    item_ordinal,
                    session_id.clone(),
                    text,
                    date.clone(),
                    mode,
                ));
            }
            LongMemEvalGranularity::Turn => {
                let mut turn_index = 0usize;
                for turn in session {
                    if turn.role != "user" {
                        continue;
                    }
                    let item_ordinal = items.len();
                    items.push(build_corpus_item(
                        &codec,
                        item_ordinal,
                        format!("{session_id}_turn_{turn_index}"),
                        turn.content.clone(),
                        date.clone(),
                        mode,
                    ));
                    turn_index += 1;
                }
            }
        }
    }

    items
}

fn build_corpus_item(
    codec: &AaakCodec,
    item_ordinal: usize,
    corpus_id: String,
    original_text: String,
    timestamp: String,
    mode: BenchMode,
) -> CorpusItem {
    let retrieval_text = match mode {
        BenchMode::Raw | BenchMode::Rooms => original_text.clone(),
        BenchMode::Aaak => codec
            .encode(
                &original_text,
                &AaakMeta {
                    wing: BENCH_WING.to_string(),
                    room: "benchmark".to_string(),
                    date: timestamp.clone(),
                    source: corpus_id.clone(),
                },
            )
            .document
            .to_string(),
    };
    let drawer_id_seed = format!("{item_ordinal}\n{corpus_id}\n{retrieval_text}");
    let drawer_id = build_drawer_id(BENCH_WING, None, &drawer_id_seed);

    CorpusItem {
        corpus_id,
        original_text,
        retrieval_text,
        timestamp,
        drawer_id,
    }
}

fn join_user_turns(session: &[LongMemEvalTurn]) -> Option<String> {
    let turns = session
        .iter()
        .filter(|turn| turn.role == "user")
        .map(|turn| turn.content.as_str())
        .collect::<Vec<_>>();
    (!turns.is_empty()).then(|| turns.join("\n"))
}

async fn ingest_corpus<E: Embedder + ?Sized>(
    db: &Database,
    embedder: &E,
    items: &[CorpusItem],
    mode: BenchMode,
) -> Result<()> {
    let texts = items
        .iter()
        .map(|item| item.retrieval_text.as_str())
        .collect::<Vec<_>>();
    let vectors = embedder
        .embed(&texts)
        .await
        .context("failed to embed benchmark corpus")?;
    let taxonomy = if mode == BenchMode::Rooms {
        db.taxonomy_entries()
            .context("failed to load rooms taxonomy for benchmark ingest")?
    } else {
        Vec::new()
    };

    for (item, vector) in items.iter().zip(vectors.iter()) {
        let room = match mode {
            BenchMode::Rooms => Some(route_room_from_taxonomy(
                &item.original_text,
                BENCH_WING,
                &taxonomy,
            )),
            BenchMode::Raw | BenchMode::Aaak => None,
        };

        db.insert_drawer(&Drawer {
            id: item.drawer_id.clone(),
            content: item.retrieval_text.clone(),
            wing: BENCH_WING.to_string(),
            room: room.clone(),
            source_file: Some(format!("longmemeval://{}", item.corpus_id)),
            source_type: SourceType::Conversation,
            added_at: item.timestamp.clone(),
            chunk_index: Some(0),
            importance: 0,
        })
        .with_context(|| format!("failed to insert drawer {}", item.drawer_id))?;
        db.insert_vector(&item.drawer_id, vector)
            .with_context(|| format!("failed to insert vector for {}", item.drawer_id))?;
    }

    Ok(())
}

fn install_rooms_taxonomy(db: &Database) -> Result<()> {
    for (room, keywords) in ROOM_KEYWORDS {
        db.upsert_taxonomy_entry(&TaxonomyEntry {
            wing: BENCH_WING.to_string(),
            room: (*room).to_string(),
            display_name: Some((*room).to_string()),
            keywords: keywords
                .iter()
                .map(|keyword| (*keyword).to_string())
                .collect(),
        })
        .with_context(|| format!("failed to install benchmark taxonomy room {room}"))?;
    }

    Ok(())
}

fn map_results_to_rankings(
    results: &[mempal::core::types::SearchResult],
    items: &[CorpusItem],
) -> Vec<usize> {
    let drawer_to_index = items
        .iter()
        .enumerate()
        .map(|(index, item)| (item.drawer_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut rankings = Vec::with_capacity(items.len());
    let mut seen = BTreeSet::new();

    for result in results {
        if let Some(&index) = drawer_to_index.get(result.drawer_id.as_str())
            && seen.insert(index)
        {
            rankings.push(index);
        }
    }

    for index in 0..items.len() {
        if seen.insert(index) {
            rankings.push(index);
        }
    }

    rankings
}

fn score_entry(
    entry: &LongMemEvalEntry,
    rankings: &[usize],
    items: &[CorpusItem],
    ks: &[usize],
) -> EntryMetricSnapshot {
    let corpus_ids = items
        .iter()
        .map(|item| item.corpus_id.clone())
        .collect::<Vec<_>>();
    let session_level_ids = corpus_ids
        .iter()
        .map(|corpus_id| session_id_from_corpus_id(corpus_id))
        .collect::<Vec<_>>();
    let answer_session_ids = entry
        .answer_session_ids
        .iter()
        .map(|id| id.as_str())
        .collect::<BTreeSet<_>>();
    let answer_turn_ids = corpus_ids
        .iter()
        .filter(|corpus_id| {
            answer_session_ids.contains(session_id_from_corpus_id(corpus_id).as_str())
        })
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut snapshot = EntryMetricSnapshot::default();

    for &k in ks {
        let (session_any, _session_all, session_ndcg) =
            evaluate_retrieval(rankings, &answer_session_ids, &session_level_ids, k);
        snapshot.session.recall_any.insert(k, session_any);
        snapshot.session.ndcg_any.insert(k, session_ndcg);

        let (turn_any, _turn_all, turn_ndcg) =
            evaluate_retrieval(rankings, &answer_turn_ids, &corpus_ids, k);
        snapshot.turn.recall_any.insert(k, turn_any);
        snapshot.turn.ndcg_any.insert(k, turn_ndcg);
    }

    snapshot
}

fn push_metric_set(target: &mut AggregateMetrics, k: usize, snapshot: &MetricSnapshot) {
    if let Some(value) = snapshot.recall_any.get(&k) {
        target.recall_any.entry(k).or_default().push(*value);
    }
    if let Some(value) = snapshot.ndcg_any.get(&k) {
        target.ndcg_any.entry(k).or_default().push(*value);
    }
    if let Some(value) = snapshot.recall_any.get(&k) {
        target.recall_all.entry(k).or_default().push(*value);
    }
}

fn summarize_metrics(metrics: &AggregateMetrics) -> MetricSnapshot {
    MetricSnapshot {
        recall_any: metrics
            .recall_any
            .iter()
            .map(|(&k, values)| (k, average(values)))
            .collect(),
        ndcg_any: metrics
            .ndcg_any
            .iter()
            .map(|(&k, values)| (k, average(values)))
            .collect(),
    }
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    values.iter().sum::<f64>() / values.len() as f64
}

fn selected_ks(top_k: usize) -> Vec<usize> {
    METRIC_KS
        .into_iter()
        .filter(|k| *k <= top_k)
        .collect::<Vec<_>>()
}

fn build_log_entry(
    entry: &LongMemEvalEntry,
    items: &[CorpusItem],
    rankings: &[usize],
    metrics: &EntryMetricSnapshot,
    top_k: usize,
) -> BenchmarkLogEntry {
    let ranked_items = rankings
        .iter()
        .take(top_k.min(items.len()))
        .map(|&index| RankedItemLog {
            corpus_id: items[index].corpus_id.clone(),
            text: truncate_log_text(&items[index].original_text),
            timestamp: items[index].timestamp.clone(),
        })
        .collect();

    BenchmarkLogEntry {
        question_id: entry.question_id.clone(),
        question_type: entry.question_type.clone(),
        question: entry.question.clone(),
        answer: entry.answer.clone(),
        retrieval_results: RetrievalLog {
            query: entry.question.clone(),
            ranked_items,
            metrics: RetrievalMetricLog {
                session: metric_map_for_log(&metrics.session),
                turn: metric_map_for_log(&metrics.turn),
            },
        },
    }
}

fn metric_map_for_log(snapshot: &MetricSnapshot) -> BTreeMap<String, f64> {
    let mut map = BTreeMap::new();
    for (&k, &value) in &snapshot.recall_any {
        map.insert(format!("recall_any@{k}"), value);
    }
    for (&k, &value) in &snapshot.ndcg_any {
        map.insert(format!("ndcg_any@{k}"), value);
    }
    map
}

fn truncate_log_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 500 {
        return compact;
    }

    compact.chars().take(500).collect::<String>()
}

fn print_summary(summary: &BenchmarkSummary) {
    println!();
    println!("============================================================");
    println!("  mempal × LongMemEval");
    println!("============================================================");
    println!("  Questions:   {}", summary.question_count);
    println!("  Mode:        {}", summary.mode.as_str());
    println!("  Granularity: {}", summary.granularity.as_str());
    println!("  Time:        {:.1}s", summary.elapsed_secs);
    println!();
    println!("  SESSION-LEVEL METRICS:");
    for (&k, &recall) in &summary.session.recall_any {
        let ndcg = summary
            .session
            .ndcg_any
            .get(&k)
            .copied()
            .unwrap_or_default();
        println!("    Recall@{k:2}: {recall:.3}    NDCG@{k:2}: {ndcg:.3}");
    }
    println!();
    println!("  TURN-LEVEL METRICS:");
    for (&k, &recall) in &summary.turn.recall_any {
        let ndcg = summary.turn.ndcg_any.get(&k).copied().unwrap_or_default();
        println!("    Recall@{k:2}: {recall:.3}    NDCG@{k:2}: {ndcg:.3}");
    }
    if !summary.per_type.is_empty() {
        println!();
        println!("  QUESTION TYPES (session Recall@5 / Recall@10 / NDCG@10):");
        for (question_type, metrics) in &summary.per_type {
            let r5 = metrics.recall_any.get(&5).copied().unwrap_or_default();
            let r10 = metrics.recall_any.get(&10).copied().unwrap_or_default();
            let nd10 = metrics.ndcg_any.get(&10).copied().unwrap_or_default();
            println!("    {question_type}: R@5={r5:.3} R@10={r10:.3} NDCG@10={nd10:.3}");
        }
    }
}

fn write_results_log(path: &Path, entries: &[BenchmarkLogEntry]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create results directory {}", parent.display()))?;
    }

    let body = entries
        .iter()
        .map(|entry| {
            serde_json::to_string(entry).context("failed to serialize LongMemEval log entry")
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    fs::write(path, body)
        .with_context(|| format!("failed to write LongMemEval results to {}", path.display()))?;
    Ok(())
}

fn dcg(relevances: &[f64], k: usize) -> f64 {
    relevances
        .iter()
        .take(k)
        .enumerate()
        .map(|(index, relevance)| relevance / ((index + 2) as f64).log2())
        .sum()
}

fn ndcg(rankings: &[usize], correct_ids: &BTreeSet<&str>, corpus_ids: &[String], k: usize) -> f64 {
    let relevances = rankings
        .iter()
        .take(k)
        .map(|&index| {
            if correct_ids.contains(corpus_ids[index].as_str()) {
                1.0
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    let mut ideal = relevances.clone();
    ideal.sort_by(|left, right| right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal));
    let ideal_dcg = dcg(&ideal, k);
    if ideal_dcg == 0.0 {
        return 0.0;
    }

    dcg(&relevances, k) / ideal_dcg
}

fn evaluate_retrieval(
    rankings: &[usize],
    correct_ids: &BTreeSet<&str>,
    corpus_ids: &[String],
    k: usize,
) -> (f64, f64, f64) {
    let top_k_ids = rankings
        .iter()
        .take(k)
        .map(|&index| corpus_ids[index].as_str())
        .collect::<BTreeSet<_>>();
    let recall_any = if correct_ids.iter().any(|id| top_k_ids.contains(id)) {
        1.0
    } else {
        0.0
    };
    let recall_all = if correct_ids.iter().all(|id| top_k_ids.contains(id)) {
        1.0
    } else {
        0.0
    };
    let ndcg = ndcg(rankings, correct_ids, corpus_ids, k);

    (recall_any, recall_all, ndcg)
}

fn session_id_from_corpus_id(corpus_id: &str) -> String {
    corpus_id
        .split_once("_turn_")
        .map(|(session_id, _)| session_id.to_string())
        .unwrap_or_else(|| corpus_id.to_string())
}

impl BenchMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Aaak => "aaak",
            Self::Rooms => "rooms",
        }
    }
}

impl LongMemEvalGranularity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Turn => "turn",
        }
    }
}

pub fn default_top_k() -> usize {
    DEFAULT_TOP_K
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestEmbedder;

    #[async_trait::async_trait]
    impl Embedder for TestEmbedder {
        async fn embed(
            &self,
            texts: &[&str],
        ) -> std::result::Result<Vec<Vec<f32>>, mempal::embed::EmbedError> {
            Ok(texts.iter().map(|text| fake_embedding(text)).collect())
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn name(&self) -> &str {
            "test"
        }
    }

    fn fake_embedding(text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0_f32; 384];
        for token in text
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            let mut hash = 0usize;
            for byte in token.to_ascii_lowercase().bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(usize::from(byte));
            }
            embedding[hash % 384] += 1.0;
        }
        embedding
    }

    fn sample_entry() -> LongMemEvalEntry {
        LongMemEvalEntry {
            question_id: "q-1".to_string(),
            question_type: "decision".to_string(),
            question: "Which auth provider did the user decide to use?".to_string(),
            answer: "Clerk".to_string(),
            haystack_sessions: vec![
                vec![
                    LongMemEvalTurn {
                        role: "user".to_string(),
                        content: "We decided to use Clerk for auth because pricing was better."
                            .to_string(),
                    },
                    LongMemEvalTurn {
                        role: "assistant".to_string(),
                        content: "I will note the auth choice.".to_string(),
                    },
                ],
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Deployment notes for Render and Postgres.".to_string(),
                }],
            ],
            haystack_session_ids: vec!["sess_auth".to_string(), "sess_deploy".to_string()],
            haystack_dates: vec!["2026-04-08".to_string(), "2026-04-09".to_string()],
            answer_session_ids: vec!["sess_auth".to_string()],
        }
    }

    fn duplicate_session_entry() -> LongMemEvalEntry {
        LongMemEvalEntry {
            question_id: "q-dup".to_string(),
            question_type: "multi-session".to_string(),
            question: "How many times did I repeat the note?".to_string(),
            answer: "2".to_string(),
            haystack_sessions: vec![
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Pick up three shirts from the store.".to_string(),
                }],
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Pick up three shirts from the store.".to_string(),
                }],
            ],
            haystack_session_ids: vec!["sess_1".to_string(), "sess_2".to_string()],
            haystack_dates: vec!["2026-04-08".to_string(), "2026-04-09".to_string()],
            answer_session_ids: vec!["sess_1".to_string(), "sess_2".to_string()],
        }
    }

    fn duplicate_session_id_entry() -> LongMemEvalEntry {
        LongMemEvalEntry {
            question_id: "q-dup-id".to_string(),
            question_type: "multi-session".to_string(),
            question: "Which repeated session should be recalled?".to_string(),
            answer: "2".to_string(),
            haystack_sessions: vec![
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Remember the pickup is at the downtown store.".to_string(),
                }],
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Remember the pickup is at the downtown store.".to_string(),
                }],
            ],
            haystack_session_ids: vec!["sess_dup".to_string(), "sess_dup".to_string()],
            haystack_dates: vec!["2026-04-08".to_string(), "2026-04-09".to_string()],
            answer_session_ids: vec!["sess_dup".to_string()],
        }
    }

    #[test]
    fn test_load_entries_accepts_numeric_answer() {
        let temp = tempdir().expect("tempdir should be created");
        let path = temp.path().join("longmemeval.json");
        fs::write(
            &path,
            r#"[{
                "question_id":"q-1",
                "question_type":"multi-session",
                "question":"How many items do I need to pick up?",
                "question_date":"2023/02/15 (Wed) 23:50",
                "answer":3,
                "answer_session_ids":["answer_1"],
                "haystack_dates":["2023/02/15 (Wed) 01:41"],
                "haystack_session_ids":["sess_1"],
                "haystack_sessions":[[
                    {"role":"user","content":"Remember to pick up three shirts."}
                ]]
            }]"#,
        )
        .expect("fixture should be written");

        let entries = load_entries(&path, 0, 0).expect("numeric answer should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].answer, "3");
    }

    #[test]
    fn test_build_corpus_assigns_unique_drawer_ids_for_duplicate_content() {
        let items = build_corpus(
            &duplicate_session_entry(),
            LongMemEvalGranularity::Session,
            BenchMode::Raw,
        );

        assert_eq!(items.len(), 2);
        assert_ne!(items[0].drawer_id, items[1].drawer_id);
    }

    #[test]
    fn test_build_corpus_assigns_unique_drawer_ids_for_duplicate_session_ids() {
        let items = build_corpus(
            &duplicate_session_id_entry(),
            LongMemEvalGranularity::Session,
            BenchMode::Raw,
        );

        assert_eq!(items.len(), 2);
        assert_ne!(items[0].drawer_id, items[1].drawer_id);
    }

    #[test]
    fn test_build_corpus_session_granularity() {
        let items = build_corpus(
            &sample_entry(),
            LongMemEvalGranularity::Session,
            BenchMode::Raw,
        );

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].corpus_id, "sess_auth");
        assert!(items[0].original_text.contains("We decided to use Clerk"));
    }

    #[test]
    fn test_build_corpus_turn_granularity() {
        let items = build_corpus(
            &sample_entry(),
            LongMemEvalGranularity::Turn,
            BenchMode::Raw,
        );

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].corpus_id, "sess_auth_turn_0");
        assert_eq!(items[1].corpus_id, "sess_deploy_turn_0");
    }

    #[test]
    fn test_build_corpus_aaak_mode_uses_encoded_text() {
        let items = build_corpus(
            &sample_entry(),
            LongMemEvalGranularity::Session,
            BenchMode::Aaak,
        );

        assert!(
            items[0]
                .retrieval_text
                .starts_with("V1|longmemeval|benchmark|")
        );
        assert!(items[0].retrieval_text.contains("Clerk"));
    }

    #[test]
    fn test_evaluate_retrieval_matches_expected_metrics() {
        let rankings = vec![1, 0, 2];
        let correct_ids = BTreeSet::from(["sess_auth"]);
        let corpus_ids = vec![
            "sess_other".to_string(),
            "sess_auth".to_string(),
            "sess_third".to_string(),
        ];

        let (recall_any, recall_all, ndcg_score) =
            evaluate_retrieval(&rankings, &correct_ids, &corpus_ids, 1);

        assert_eq!(recall_any, 1.0);
        assert_eq!(recall_all, 1.0);
        assert!((ndcg_score - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_run_benchmark_raw_small_sample() {
        let args = LongMemEvalArgs {
            data_file: PathBuf::from("unused.json"),
            mode: BenchMode::Raw,
            granularity: LongMemEvalGranularity::Session,
            limit: 0,
            skip: 0,
            top_k: 5,
            out: None,
        };
        let entry = sample_entry();

        let (summary, logs) = run_benchmark_with_embedder(&TestEmbedder, &[entry], &args)
            .await
            .expect("benchmark should run");

        assert_eq!(summary.question_count, 1);
        assert_eq!(summary.session.recall_any.get(&1), Some(&1.0));
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].retrieval_results.metrics.session["recall_any@1"],
            1.0
        );
    }

    #[tokio::test]
    async fn test_run_benchmark_rooms_small_sample() {
        let args = LongMemEvalArgs {
            data_file: PathBuf::from("unused.json"),
            mode: BenchMode::Rooms,
            granularity: LongMemEvalGranularity::Session,
            limit: 0,
            skip: 0,
            top_k: 5,
            out: None,
        };
        let entry = sample_entry();

        let (summary, _logs) = run_benchmark_with_embedder(&TestEmbedder, &[entry], &args)
            .await
            .expect("rooms benchmark should run");

        assert_eq!(summary.question_count, 1);
        assert_eq!(summary.session.recall_any.get(&1), Some(&1.0));
    }
}
