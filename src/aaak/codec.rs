use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use bimap::BiHashMap;
use jieba_rs::Jieba;

fn jieba() -> &'static Jieba {
    static INSTANCE: OnceLock<Jieba> = OnceLock::new();
    INSTANCE.get_or_init(Jieba::new)
}

use super::model::{
    AaakDocument, AaakHeader, AaakLine, AaakMeta, EncodeOutput, EncodeReport, RoundtripReport,
    Zettel,
};

const DEFAULT_MAX_TOPICS: usize = 3;
const DEFAULT_EMOTION: &str = "determ";
const DEFAULT_ENTITY_CODE: &str = "UNK";
pub(crate) const EMOTION_SIGNALS: &[(&str, &str)] = &[
    ("decided", "determ"),
    ("determined", "determ"),
    ("prefer", "convict"),
    ("confident", "convict"),
    ("worried", "anx"),
    ("anxious", "anx"),
    ("concern", "anx"),
    ("excited", "excite"),
    ("frustrated", "frust"),
    ("confused", "confuse"),
    ("love", "love"),
    ("hate", "rage"),
    ("hope", "hope"),
    ("fear", "fear"),
    ("trust", "trust"),
    ("happy", "joy"),
    ("joy", "joy"),
    ("sad", "grief"),
    ("grief", "grief"),
    ("surprised", "surpr"),
    ("grateful", "grat"),
    ("curious", "curious"),
    ("wonder", "wonder"),
    ("relieved", "relief"),
    ("satisf", "satis"),
    ("disappoint", "grief"),
    ("vulnerable", "vul"),
    ("tender", "tender"),
    ("honest", "raw"),
    ("doubt", "doubt"),
    ("exhaust", "exhaust"),
    ("warm", "warmth"),
    ("humor", "humor"),
    ("funny", "humor"),
    ("peace", "peace"),
    ("despair", "despair"),
    ("passion", "passion"),
    // Chinese
    ("\u{51b3}\u{5b9a}", "determ"),  // 决定
    ("\u{786e}\u{5b9a}", "determ"),  // 确定
    ("\u{62c5}\u{5fc3}", "anx"),     // 担心
    ("\u{7126}\u{8651}", "anx"),     // 焦虑
    ("\u{5174}\u{594b}", "excite"),  // 兴奋
    ("\u{6cae}\u{4e27}", "frust"),   // 沮丧
    ("\u{56f0}\u{60d1}", "confuse"), // 困惑
    ("\u{5f00}\u{5fc3}", "joy"),     // 开心
    ("\u{9ad8}\u{5174}", "joy"),     // 高兴
    ("\u{60b2}\u{4f24}", "grief"),   // 悲伤
    ("\u{60ca}\u{8bb6}", "surpr"),   // 惊讶
    ("\u{611f}\u{6069}", "grat"),    // 感恩
    ("\u{611f}\u{8c22}", "grat"),    // 感谢
    ("\u{597d}\u{5947}", "curious"), // 好奇
    ("\u{4fe1}\u{4efb}", "trust"),   // 信任
    ("\u{5e0c}\u{671b}", "hope"),    // 希望
    ("\u{6050}\u{60e7}", "fear"),    // 恐惧
    ("\u{5bb3}\u{6015}", "fear"),    // 害怕
    ("\u{6ee1}\u{610f}", "satis"),   // 满意
    ("\u{5931}\u{671b}", "grief"),   // 失望
    ("\u{8f7b}\u{677e}", "relief"),  // 轻松
    ("\u{653e}\u{5fc3}", "relief"),  // 放心
    ("\u{7231}", "love"),            // 爱
    ("\u{6068}", "rage"),            // 恨
    ("\u{6016}\u{60e7}", "fear"),    // 恐惧
    ("\u{5e73}\u{9759}", "peace"),   // 平静
    ("\u{7edd}\u{671b}", "despair"), // 绝望
    ("\u{70ed}\u{60c5}", "passion"), // 热情
    ("\u{6000}\u{7591}", "doubt"),   // 怀疑
    ("\u{75b2}\u{60eb}", "exhaust"), // 疲惫
];
pub(crate) const FLAG_SIGNALS: &[(&str, &str)] = &[
    ("decid", "DECISION"),
    ("chose", "DECISION"),
    ("switch", "DECISION"),
    ("migrat", "DECISION"),
    ("replace", "DECISION"),
    ("recommend", "DECISION"),
    ("because", "DECISION"),
    ("found", "ORIGIN"),
    ("create", "ORIGIN"),
    ("start", "ORIGIN"),
    ("born", "ORIGIN"),
    ("launch", "ORIGIN"),
    ("first time", "ORIGIN"),
    ("core", "CORE"),
    ("fundamental", "CORE"),
    ("essential", "CORE"),
    ("principle", "CORE"),
    ("belief", "CORE"),
    ("always", "CORE"),
    ("turning point", "PIVOT"),
    ("changed everything", "PIVOT"),
    ("realized", "PIVOT"),
    ("breakthrough", "PIVOT"),
    ("epiphany", "PIVOT"),
    ("api", "TECHNICAL"),
    ("database", "TECHNICAL"),
    ("architecture", "TECHNICAL"),
    ("deploy", "TECHNICAL"),
    ("infrastructure", "TECHNICAL"),
    ("framework", "TECHNICAL"),
    ("server", "TECHNICAL"),
    ("config", "TECHNICAL"),
    ("auth", "TECHNICAL"),
    ("token", "SENSITIVE"),
    ("password", "SENSITIVE"),
    ("secret", "SENSITIVE"),
    ("credential", "SENSITIVE"),
    ("private", "SENSITIVE"),
    ("sensitive", "SENSITIVE"),
    ("pii", "SENSITIVE"),
    // Chinese
    ("\u{51b3}\u{5b9a}", "DECISION"),          // 决定
    ("\u{9009}\u{62e9}", "DECISION"),          // 选择
    ("\u{5207}\u{6362}", "DECISION"),          // 切换
    ("\u{8fc1}\u{79fb}", "DECISION"),          // 迁移
    ("\u{66ff}\u{6362}", "DECISION"),          // 替换
    ("\u{63a8}\u{8350}", "DECISION"),          // 推荐
    ("\u{56e0}\u{4e3a}", "DECISION"),          // 因为
    ("\u{521b}\u{5efa}", "ORIGIN"),            // 创建
    ("\u{521b}\u{7acb}", "ORIGIN"),            // 创立
    ("\u{5f00}\u{59cb}", "ORIGIN"),            // 开始
    ("\u{7b2c}\u{4e00}\u{6b21}", "ORIGIN"),    // 第一次
    ("\u{6838}\u{5fc3}", "CORE"),              // 核心
    ("\u{57fa}\u{672c}", "CORE"),              // 基本
    ("\u{539f}\u{5219}", "CORE"),              // 原则
    ("\u{4fe1}\u{5ff5}", "CORE"),              // 信念
    ("\u{8f6c}\u{6298}", "PIVOT"),             // 转折
    ("\u{7a81}\u{7834}", "PIVOT"),             // 突破
    ("\u{987f}\u{609f}", "PIVOT"),             // 顿悟
    ("\u{63a5}\u{53e3}", "TECHNICAL"),         // 接口
    ("\u{6570}\u{636e}\u{5e93}", "TECHNICAL"), // 数据库
    ("\u{67b6}\u{6784}", "TECHNICAL"),         // 架构
    ("\u{90e8}\u{7f72}", "TECHNICAL"),         // 部署
    ("\u{6846}\u{67b6}", "TECHNICAL"),         // 框架
    ("\u{670d}\u{52a1}\u{5668}", "TECHNICAL"), // 服务器
    ("\u{914d}\u{7f6e}", "TECHNICAL"),         // 配置
    ("\u{8ba4}\u{8bc1}", "TECHNICAL"),         // 认证
    ("\u{5bc6}\u{7801}", "SENSITIVE"),         // 密码
    ("\u{5bc6}\u{94a5}", "SENSITIVE"),         // 密钥
    ("\u{51ed}\u{8bc1}", "SENSITIVE"),         // 凭证
    ("\u{9690}\u{79c1}", "SENSITIVE"),         // 隐私
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextSegmentKind {
    AsciiWord,
    Cjk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSegment {
    kind: TextSegmentKind,
    text: String,
}

#[derive(Debug, Clone)]
pub struct AaakCodec {
    entity_map: BiHashMap<String, String>,
    max_topics: usize,
}

impl Default for AaakCodec {
    fn default() -> Self {
        Self {
            entity_map: BiHashMap::new(),
            max_topics: DEFAULT_MAX_TOPICS,
        }
    }
}

impl AaakCodec {
    pub fn with_entity_aliases(aliases: BTreeMap<String, String>) -> Self {
        let entity_map = aliases.into_iter().collect::<BiHashMap<_, _>>();
        Self {
            entity_map,
            max_topics: DEFAULT_MAX_TOPICS,
        }
    }

    pub fn encode(&self, text: &str, meta: &AaakMeta) -> EncodeOutput {
        let normalized = normalize_whitespace(text);
        let all_topics = extract_topics(&normalized);
        let topics = all_topics
            .iter()
            .take(self.max_topics)
            .cloned()
            .collect::<Vec<_>>();
        let topics_truncated = all_topics.len().saturating_sub(self.max_topics);
        let mut entity_codes = Vec::new();
        let mut seen_codes = BTreeSet::new();

        // First: match known entity_map keys against text
        for (name, code) in self.entity_map.iter() {
            if normalized.contains(name.as_str()) && seen_codes.insert(code.clone()) {
                entity_codes.push(code.clone());
            }
        }

        // Then: auto-detect entities from text
        for entity in extract_entities(&normalized) {
            let code = self.entity_code(&entity);
            if seen_codes.insert(code.clone()) {
                entity_codes.push(code);
            }
        }

        let entities = if entity_codes.is_empty() {
            vec![DEFAULT_ENTITY_CODE.to_string()]
        } else {
            entity_codes
        };
        let flags = detect_flags(&normalized);
        let emotions = detect_emotions(&normalized);

        let zettel = Zettel {
            id: 0,
            entities,
            topics: if topics.is_empty() {
                vec!["note".to_string()]
            } else {
                topics
            },
            quote: normalized,
            weight: infer_weight(&flags),
            emotions,
            flags,
        };
        let document = AaakDocument {
            header: AaakHeader {
                version: 1,
                wing: meta.wing.clone(),
                room: meta.room.clone(),
                date: meta.date.clone(),
                source: meta.source.clone(),
            },
            body: vec![AaakLine::Zettel(zettel.clone())],
            zettels: vec![zettel],
        };
        let roundtrip = self.verify_roundtrip(text, &document);

        EncodeOutput {
            document,
            report: EncodeReport {
                topics_truncated,
                key_sentence_truncated: false,
                coverage: roundtrip.coverage,
                lost_assertions: roundtrip.lost,
            },
        }
    }

    pub fn decode(&self, document: &AaakDocument) -> String {
        document
            .zettel_lines()
            .iter()
            .map(|zettel| self.decode_zettel(zettel))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn verify_roundtrip(&self, original: &str, document: &AaakDocument) -> RoundtripReport {
        let decoded = normalize_whitespace(&self.decode(document)).to_lowercase();
        let assertions = split_assertions(original);
        if assertions.is_empty() {
            return RoundtripReport {
                preserved: Vec::new(),
                lost: Vec::new(),
                coverage: 1.0,
            };
        }

        let (preserved, lost): (Vec<_>, Vec<_>) = assertions
            .into_iter()
            .partition(|assertion| decoded.contains(&assertion.to_lowercase()));
        let coverage = preserved.len() as f32 / (preserved.len() + lost.len()) as f32;

        RoundtripReport {
            preserved,
            lost,
            coverage,
        }
    }

    fn decode_zettel(&self, zettel: &Zettel) -> String {
        let mut quote = zettel.quote.clone();
        for entity in &zettel.entities {
            if let Some(name) = self.entity_map.get_by_right(entity) {
                quote = replace_code(&quote, entity, name);
            }
        }

        if quote.is_empty() {
            return zettel
                .entities
                .iter()
                .map(|entity| {
                    self.entity_map
                        .get_by_right(entity)
                        .cloned()
                        .unwrap_or_else(|| entity.clone())
                })
                .collect::<Vec<_>>()
                .join(" ");
        }

        quote
    }

    fn entity_code(&self, entity: &str) -> String {
        self.entity_map
            .get_by_left(entity)
            .cloned()
            .unwrap_or_else(|| default_entity_code(entity))
    }
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'")
        .trim()
        .to_string()
}

fn extract_entities(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut entities = Vec::new();

    for segment in text_segments(text) {
        match segment.kind {
            TextSegmentKind::AsciiWord => {
                if looks_like_ascii_entity(&segment.text) && seen.insert(segment.text.clone()) {
                    entities.push(segment.text);
                }
            }
            TextSegmentKind::Cjk => {
                for candidate in extract_cjk_entities_from_segment(&segment.text) {
                    if seen.insert(candidate.clone()) {
                        entities.push(candidate);
                    }
                }
            }
        }
    }

    entities
}

fn extract_topics(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "with", "over", "based", "this", "that", "was", "use", "why",
    ];

    let mut seen = BTreeSet::new();
    let mut topics = Vec::new();

    for segment in text_segments(text) {
        match segment.kind {
            TextSegmentKind::AsciiWord => {
                let token = segment.text.to_lowercase();
                if token.is_empty() || STOP_WORDS.contains(&token.as_str()) {
                    continue;
                }

                if seen.insert(token.clone()) {
                    topics.push(token);
                }
            }
            TextSegmentKind::Cjk => {
                for topic in extract_cjk_topics_from_segment(&segment.text) {
                    if seen.insert(topic.clone()) {
                        topics.push(topic);
                    }
                }
            }
        }
    }

    topics
}

fn detect_flags(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut flags = Vec::new();

    for (needle, flag) in FLAG_SIGNALS {
        if lower.contains(needle) && !flags.iter().any(|existing| existing == flag) {
            flags.push((*flag).to_string());
        }
    }
    if flags.is_empty() {
        flags.push("CORE".to_string());
    }

    flags
}

fn detect_emotions(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut emotions = Vec::new();

    for (needle, emotion) in EMOTION_SIGNALS {
        if lower.contains(needle) && !emotions.iter().any(|existing| existing == emotion) {
            emotions.push((*emotion).to_string());
        }
    }

    if emotions.is_empty() {
        emotions.push(DEFAULT_EMOTION.to_string());
    }

    emotions
}

fn infer_weight(flags: &[String]) -> u8 {
    if flags
        .iter()
        .any(|flag| flag == "DECISION" || flag == "PIVOT")
    {
        4
    } else if flags.iter().any(|flag| flag == "TECHNICAL") {
        3
    } else {
        2
    }
}

fn split_assertions(text: &str) -> Vec<String> {
    text.split([
        '.', '!', '?', ';', '\u{3002}', // 。
        '\u{FF01}', // ！
        '\u{FF1F}', // ？
        '\u{FF1B}', // ；
        '\u{FF0C}', // ，
    ])
    .map(normalize_whitespace)
    .filter(|item| !item.is_empty())
    .collect()
}

fn replace_code(quote: &str, code: &str, name: &str) -> String {
    let mut replaced = String::with_capacity(quote.len());
    let mut token = String::new();

    for ch in quote.chars() {
        if ch.is_ascii_alphanumeric() {
            token.push(ch);
            continue;
        }

        push_replacement(&mut replaced, &mut token, code, name);
        replaced.push(ch);
    }

    push_replacement(&mut replaced, &mut token, code, name);
    replaced
}

fn default_entity_code(entity: &str) -> String {
    let ascii_code: String = entity
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .take(3)
        .collect::<String>()
        .to_uppercase();

    if ascii_code.len() >= 3 {
        return ascii_code;
    }

    // For non-ASCII entities (e.g., Chinese), generate a stable hash-based code
    let hash = stable_hash(entity);
    let mut code = String::with_capacity(3);
    for i in 0..3u64 {
        let byte = ((hash >> (i * 5)) & 0x1F) as u8;
        code.push((b'A' + byte % 26) as char);
    }
    code
}

fn stable_hash(s: &str) -> u64 {
    let mut h: u64 = 0;
    for byte in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    h
}

fn is_cjk_ideograph(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
    )
}

fn text_segments(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_kind = None;

    for ch in text.chars() {
        let next_kind = if ch.is_ascii_alphanumeric() {
            Some(TextSegmentKind::AsciiWord)
        } else if is_cjk_ideograph(ch) {
            Some(TextSegmentKind::Cjk)
        } else {
            None
        };

        match (current_kind, next_kind) {
            (Some(kind), Some(next)) if kind == next => current.push(ch),
            (Some(kind), Some(next)) => {
                segments.push(TextSegment {
                    kind,
                    text: std::mem::take(&mut current),
                });
                current.push(ch);
                current_kind = Some(next);
            }
            (Some(kind), None) => {
                if !current.is_empty() {
                    segments.push(TextSegment {
                        kind,
                        text: std::mem::take(&mut current),
                    });
                }
                current_kind = None;
            }
            (None, Some(next)) => {
                current.push(ch);
                current_kind = Some(next);
            }
            (None, None) => {}
        }
    }

    if let Some(kind) = current_kind
        && !current.is_empty()
    {
        segments.push(TextSegment {
            kind,
            text: current,
        });
    }

    segments
}

fn looks_like_ascii_entity(token: &str) -> bool {
    token.len() >= 2
        && token
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
}

/// Extract CJK entities using jieba POS tagging.
///
/// Identifies proper nouns via part-of-speech tags:
/// - `nr` = person name
/// - `ns` = place name
/// - `nt` = organization name
/// - `nz` = other proper noun
///
/// This replaces the earlier 2-4 char heuristic and can handle names of
/// arbitrary length that jieba's default dictionary knows about.
fn extract_cjk_entities_from_segment(segment: &str) -> Vec<String> {
    let mut entities = Vec::new();
    let mut seen = BTreeSet::new();

    for tag in jieba().tag(segment, true) {
        // nr/nrfg/nrt = person name variants, ns = place, nt = org, nz = other proper
        let is_entity =
            tag.tag.starts_with("nr") || tag.tag == "ns" || tag.tag == "nt" || tag.tag == "nz";
        if !is_entity {
            continue;
        }
        if tag.word.chars().count() < 2 {
            continue;
        }
        if seen.insert(tag.word.to_string()) {
            entities.push(tag.word.to_string());
        }
    }

    entities
}

/// Extract CJK topics using jieba word segmentation with POS filtering.
///
/// Keeps content words (nouns, verbs, adjectives) of length >= 2, filters
/// out function words (particles, conjunctions, pronouns) and proper nouns
/// (which are handled by `extract_cjk_entities_from_segment` instead).
fn extract_cjk_topics_from_segment(segment: &str) -> Vec<String> {
    let mut topics = Vec::new();
    let mut seen = BTreeSet::new();

    for tag in jieba().tag(segment, true) {
        // Skip proper nouns — they go to entities, not topics
        if tag.tag.starts_with("nr") || tag.tag == "ns" || tag.tag == "nt" || tag.tag == "nz" {
            continue;
        }
        // Keep content-bearing POS: nouns (n*), verbs (v*), adjectives (a*)
        let first = tag.tag.chars().next().unwrap_or(' ');
        if !matches!(first, 'n' | 'v' | 'a') {
            continue;
        }
        if tag.word.chars().count() < 2 {
            continue;
        }
        // Skip pronouns like 我们/你们/这个 (POS = r, but some dicts tag them as n)
        if is_cjk_function_word(tag.word) {
            continue;
        }
        if seen.insert(tag.word.to_string()) {
            topics.push(tag.word.to_string());
        }
    }

    topics
}

fn is_cjk_function_word(word: &str) -> bool {
    // Fallback filter for words jieba tags as nominal but are actually
    // pronouns/demonstratives/conjunctions.
    matches!(
        word,
        "\u{6211}\u{4eec}" // 我们
            | "\u{4f60}\u{4eec}" // 你们
            | "\u{4ed6}\u{4eec}" // 他们
            | "\u{5979}\u{4eec}" // 她们
            | "\u{5b83}\u{4eec}" // 它们
            | "\u{8fd9}\u{4e2a}" // 这个
            | "\u{90a3}\u{4e2a}" // 那个
            | "\u{8fd9}\u{4e9b}" // 这些
            | "\u{90a3}\u{4e9b}" // 那些
            | "\u{4e3a}\u{4ec0}\u{4e48}" // 为什么
            | "\u{600e}\u{4e48}" // 怎么
            | "\u{4ec0}\u{4e48}" // 什么
            | "\u{56e0}\u{4e3a}" // 因为
            | "\u{6240}\u{4ee5}" // 所以
            | "\u{4f46}\u{662f}" // 但是
    )
}

fn push_replacement(output: &mut String, token: &mut String, code: &str, name: &str) {
    if token.is_empty() {
        return;
    }

    if token == code {
        output.push_str(name);
    } else {
        output.push_str(token);
    }
    token.clear();
}
