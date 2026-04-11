use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::parse::parse_document;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AaakMeta {
    pub wing: String,
    pub room: String,
    pub date: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AaakHeader {
    pub version: u8,
    pub wing: String,
    pub room: String,
    pub date: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zettel {
    pub id: usize,
    pub entities: Vec<String>,
    pub topics: Vec<String>,
    pub quote: String,
    pub weight: u8,
    pub emotions: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tunnel {
    pub left: usize,
    pub right: usize,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArcLine {
    pub emotions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AaakLine {
    Zettel(Zettel),
    Tunnel(Tunnel),
    Arc(ArcLine),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AaakDocument {
    pub header: AaakHeader,
    pub body: Vec<AaakLine>,
    pub zettels: Vec<Zettel>,
}

impl AaakDocument {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse_document(input)
    }

    pub fn zettel_lines(&self) -> Vec<Zettel> {
        if !self.body.is_empty() {
            return self
                .body
                .iter()
                .filter_map(|line| match line {
                    AaakLine::Zettel(zettel) => Some(zettel.clone()),
                    AaakLine::Tunnel(_) | AaakLine::Arc(_) => None,
                })
                .collect();
        }

        self.zettels.clone()
    }
}

impl Display for AaakDocument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "V{}|{}|{}|{}|{}",
            self.header.version,
            self.header.wing,
            self.header.room,
            self.header.date,
            self.header.source
        )?;

        for (index, line) in self.body_lines().iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            match line {
                AaakLine::Zettel(zettel) => write!(
                    f,
                    "{}:{}|{}|\"{}\"|{}|{}|{}",
                    zettel.id,
                    zettel.entities.join("+"),
                    zettel.topics.join("_"),
                    zettel.quote.replace('"', "'"),
                    "★".repeat(usize::from(zettel.weight.max(1))),
                    zettel.emotions.join("+"),
                    zettel.flags.join("+"),
                )?,
                AaakLine::Tunnel(tunnel) => {
                    write!(f, "T:{}<->{}|{}", tunnel.left, tunnel.right, tunnel.label)?
                }
                AaakLine::Arc(arc) => write!(f, "ARC:{}", arc.emotions.join("->"))?,
            }
        }

        Ok(())
    }
}

impl AaakDocument {
    fn body_lines(&self) -> Vec<AaakLine> {
        if !self.body.is_empty() {
            return self.body.clone();
        }

        self.zettels.iter().cloned().map(AaakLine::Zettel).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncodeReport {
    pub topics_truncated: usize,
    pub key_sentence_truncated: bool,
    pub coverage: f32,
    pub lost_assertions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncodeOutput {
    pub document: AaakDocument,
    pub report: EncodeReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundtripReport {
    pub preserved: Vec<String>,
    pub lost: Vec<String>,
    pub coverage: f32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("missing AAAK header")]
    MissingHeader,
    #[error("invalid AAAK header")]
    InvalidHeader,
    #[error("invalid version marker")]
    InvalidVersion,
    #[error("invalid zettel line: {0}")]
    InvalidZettel(String),
}
