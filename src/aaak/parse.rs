use std::collections::BTreeSet;

use super::model::{AaakDocument, AaakHeader, AaakLine, ArcLine, ParseError, Tunnel, Zettel};

pub(crate) const ALLOWED_FLAGS: &[&str] = &[
    "DECISION",
    "ORIGIN",
    "CORE",
    "PIVOT",
    "TECHNICAL",
    "SENSITIVE",
];

pub fn parse_document(input: &str) -> Result<AaakDocument, ParseError> {
    let mut lines = input.lines().filter(|line| !line.trim().is_empty());
    let header = parse_header(lines.next().ok_or(ParseError::MissingHeader)?)?;
    let mut body = Vec::new();
    let mut zettels = Vec::new();
    let mut zettel_ids = BTreeSet::new();

    for line in lines {
        if line.starts_with("T:") {
            body.push(AaakLine::Tunnel(parse_tunnel(line)?));
            continue;
        }
        if line.starts_with("ARC:") {
            body.push(AaakLine::Arc(parse_arc(line)?));
            continue;
        }
        let zettel = parse_zettel(line)?;
        if !zettel_ids.insert(zettel.id) {
            return Err(ParseError::InvalidZettel(line.to_string()));
        }
        body.push(AaakLine::Zettel(zettel.clone()));
        zettels.push(zettel);
    }

    for line in &body {
        if let AaakLine::Tunnel(tunnel) = line
            && (!zettel_ids.contains(&tunnel.left) || !zettel_ids.contains(&tunnel.right))
        {
            return Err(ParseError::InvalidZettel(format!(
                "T:{}<->{}|{}",
                tunnel.left, tunnel.right, tunnel.label
            )));
        }
    }

    Ok(AaakDocument {
        header,
        body,
        zettels,
    })
}

fn parse_header(line: &str) -> Result<AaakHeader, ParseError> {
    let mut parts = line.split('|');
    let version = parts.next().ok_or(ParseError::InvalidHeader)?;
    let wing = parts.next().ok_or(ParseError::InvalidHeader)?;
    let room = parts.next().ok_or(ParseError::InvalidHeader)?;
    let date = parts.next().ok_or(ParseError::InvalidHeader)?;
    let source = parts.next().ok_or(ParseError::InvalidHeader)?;
    if parts.next().is_some() {
        return Err(ParseError::InvalidHeader);
    }

    let version = version
        .strip_prefix('V')
        .ok_or(ParseError::InvalidVersion)?
        .parse::<u8>()
        .map_err(|_| ParseError::InvalidVersion)?;

    Ok(AaakHeader {
        version,
        wing: wing.to_string(),
        room: room.to_string(),
        date: date.to_string(),
        source: source.to_string(),
    })
}

fn parse_zettel(line: &str) -> Result<Zettel, ParseError> {
    let (id, rest) = line
        .split_once(':')
        .ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    let id = id
        .parse::<usize>()
        .map_err(|_| ParseError::InvalidZettel(line.to_string()))?;
    let parts = rest.split('|').collect::<Vec<_>>();
    if parts.len() != 6 {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let entities = split_field(parts[0], '+');
    if entities.is_empty() || entities.iter().any(|entity| !is_entity_code(entity)) {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let topics = split_field(parts[1], '_');
    if topics.is_empty() {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let quote = parse_quote(parts[2]).ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    let weight =
        parse_weight(parts[3]).ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;

    let emotions = split_field(parts[4], '+');
    if emotions.is_empty() || emotions.iter().any(|emotion| !is_emotion_code(emotion)) {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let flags = split_field(parts[5], '+');
    if flags.is_empty()
        || flags
            .iter()
            .any(|flag| !ALLOWED_FLAGS.contains(&flag.as_str()))
    {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    Ok(Zettel {
        id,
        entities,
        topics,
        quote,
        weight,
        emotions,
        flags,
    })
}

fn parse_tunnel(line: &str) -> Result<Tunnel, ParseError> {
    let rest = line
        .strip_prefix("T:")
        .ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    let (pair, label) = rest
        .split_once('|')
        .ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    if label.trim().is_empty() {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let (left, right) = pair
        .split_once("<->")
        .ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    let left = left
        .parse::<usize>()
        .map_err(|_| ParseError::InvalidZettel(line.to_string()))?;
    let right = right
        .parse::<usize>()
        .map_err(|_| ParseError::InvalidZettel(line.to_string()))?;

    Ok(Tunnel {
        left,
        right,
        label: label.to_string(),
    })
}

fn parse_arc(line: &str) -> Result<ArcLine, ParseError> {
    let rest = line
        .strip_prefix("ARC:")
        .ok_or_else(|| ParseError::InvalidZettel(line.to_string()))?;
    if rest.is_empty() {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    let emotions = rest.split("->").collect::<Vec<_>>();
    if emotions.is_empty() || emotions.iter().any(|emotion| !is_emotion_code(emotion)) {
        return Err(ParseError::InvalidZettel(line.to_string()));
    }

    Ok(ArcLine {
        emotions: emotions.into_iter().map(ToOwned::to_owned).collect(),
    })
}

fn split_field(raw: &str, separator: char) -> Vec<String> {
    raw.split(separator)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_quote(raw: &str) -> Option<String> {
    if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
        return None;
    }

    Some(raw[1..raw.len() - 1].to_string())
}

fn parse_weight(raw: &str) -> Option<u8> {
    if raw.is_empty() || !raw.chars().all(|ch| ch == '★') {
        return None;
    }

    let count = raw.chars().count();
    (1..=5)
        .contains(&count)
        .then(|| u8::try_from(count).ok())
        .flatten()
}

fn is_entity_code(raw: &str) -> bool {
    raw.len() == 3 && raw.chars().all(|ch| ch.is_ascii_uppercase())
}

fn is_emotion_code(raw: &str) -> bool {
    (3..=7).contains(&raw.len()) && raw.chars().all(|ch| ch.is_ascii_lowercase())
}
