//! Heuristic text → (subject, predicate, object) extraction.
//!
//! Narrow pattern matching only — no regex crate, no NLP. Unknown shapes
//! return empty. Tokenisation is whitespace split + trailing punctuation
//! strip. Subject / object must be ASCII Capitalized proper nouns.

/// Strip trailing ASCII punctuation commonly attached to words
/// (`.`, `,`, `;`, `:`, `?`, `!`, `)`, `]`, `"`). Leading punctuation is
/// rare in proper-noun context and left alone for simplicity.
fn trim_trailing_punct(word: &str) -> &str {
    word.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '?' | '!' | ')' | ']' | '"' | '\''
        )
    })
}

/// Same as trim_trailing_punct but also trims a trailing "'s" possessive.
fn strip_possessive(word: &str) -> (&str, bool) {
    let trimmed = trim_trailing_punct(word);
    if let Some(stem) = trimmed.strip_suffix("'s") {
        (stem, true)
    } else {
        (trimmed, false)
    }
}

fn is_proper_noun(word: &str) -> bool {
    let stripped = trim_trailing_punct(word);
    stripped.len() >= 3
        && stripped.chars().all(|c| c.is_ascii_alphabetic())
        && stripped
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
}

fn is_lowercase_word(word: &str) -> bool {
    let stripped = trim_trailing_punct(word);
    !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_lowercase())
}

/// Extract candidate (subject, predicate, object) triples from text.
pub fn extract_triples(text: &str) -> Vec<(String, String, String)> {
    let mut triples = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    for i in 0..words.len() {
        // Pattern 1: "X is Y's ROLE" → (X, ROLE_of, Y)
        // Requires i+3 tokens after i.
        if let Some(triple) = try_possessive(&words, i) {
            push_unique(&mut triples, triple);
        }

        // Pattern 2: "X works at Y" or "X works for Y"
        if let Some(triple) = try_works_at(&words, i) {
            push_unique(&mut triples, triple);
        }

        // Pattern 3: "X is [the/a/an] ROLE of Y"
        if let Some(triple) = try_role_of(&words, i) {
            push_unique(&mut triples, triple);
        }
    }
    triples
}

fn push_unique(bucket: &mut Vec<(String, String, String)>, t: (String, String, String)) {
    if !bucket.contains(&t) {
        bucket.push(t);
    }
}

/// "X is Y's ROLE" → (X, ROLE_of, Y)
fn try_possessive(words: &[&str], i: usize) -> Option<(String, String, String)> {
    if i + 3 >= words.len() {
        return None;
    }
    let subject_raw = words[i];
    let is_word = trim_trailing_punct(words[i + 1]);
    let maybe_possessive = words[i + 2];
    let role_raw = words[i + 3];

    if !is_proper_noun(subject_raw) {
        return None;
    }
    if is_word != "is" {
        return None;
    }
    let (object_stem, has_apos_s) = strip_possessive(maybe_possessive);
    if !has_apos_s || !is_proper_noun(object_stem) {
        return None;
    }
    let role = trim_trailing_punct(role_raw);
    if !is_lowercase_word(role) {
        return None;
    }
    Some((
        trim_trailing_punct(subject_raw).to_string(),
        format!("{}_of", role.to_lowercase()),
        object_stem.to_string(),
    ))
}

/// "X works at Y" or "X works for Y" → (X, works_at, Y)
fn try_works_at(words: &[&str], i: usize) -> Option<(String, String, String)> {
    if i + 3 >= words.len() {
        return None;
    }
    let subject = words[i];
    let verb = trim_trailing_punct(words[i + 1]);
    let preposition = trim_trailing_punct(words[i + 2]);
    let object = words[i + 3];

    if !is_proper_noun(subject) {
        return None;
    }
    if verb != "works" && verb != "work" {
        return None;
    }
    if preposition != "at" && preposition != "for" {
        return None;
    }
    if !is_proper_noun(object) {
        return None;
    }
    Some((
        trim_trailing_punct(subject).to_string(),
        "works_at".to_string(),
        trim_trailing_punct(object).to_string(),
    ))
}

/// "X is [the|a|an] ROLE of Y" → (X, ROLE_of, Y)
fn try_role_of(words: &[&str], i: usize) -> Option<(String, String, String)> {
    if i + 4 >= words.len() {
        return None;
    }
    let subject = words[i];
    let is_word = trim_trailing_punct(words[i + 1]);
    let mut idx = i + 2;
    if idx >= words.len() {
        return None;
    }
    let article = trim_trailing_punct(words[idx]);
    if matches!(article, "the" | "a" | "an") {
        idx += 1;
    }
    if idx + 2 >= words.len() {
        return None;
    }
    let role = trim_trailing_punct(words[idx]);
    let of = trim_trailing_punct(words[idx + 1]);
    let object = words[idx + 2];

    if !is_proper_noun(subject) || is_word != "is" {
        return None;
    }
    if !is_lowercase_word(role) || of != "of" || !is_proper_noun(object) {
        return None;
    }
    Some((
        trim_trailing_punct(subject).to_string(),
        format!("{}_of", role.to_lowercase()),
        trim_trailing_punct(object).to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extracts_possessive_relation() {
        let triples = extract_triples("Bob is Alice's brother");
        assert_eq!(
            triples,
            vec![(
                "Bob".to_string(),
                "brother_of".to_string(),
                "Alice".to_string()
            )]
        );
    }

    #[test]
    fn test_extracts_works_at() {
        let triples = extract_triples("Alice works at Acme");
        assert!(triples.contains(&(
            "Alice".to_string(),
            "works_at".to_string(),
            "Acme".to_string()
        )));
    }

    #[test]
    fn test_extracts_works_for_variant() {
        let triples = extract_triples("Alice works for Acme");
        assert!(triples.contains(&(
            "Alice".to_string(),
            "works_at".to_string(),
            "Acme".to_string()
        )));
    }

    #[test]
    fn test_extracts_role_of_with_article() {
        let triples = extract_triples("Alice is the founder of Acme");
        assert!(triples.contains(&(
            "Alice".to_string(),
            "founder_of".to_string(),
            "Acme".to_string()
        )));
    }

    #[test]
    fn test_extracts_role_of_without_article() {
        let triples = extract_triples("Bob is founder of Acme");
        assert!(triples.contains(&(
            "Bob".to_string(),
            "founder_of".to_string(),
            "Acme".to_string()
        )));
    }

    #[test]
    fn test_unknown_sentence_returns_empty() {
        let triples = extract_triples("Bob and Alice went hiking.");
        assert!(triples.is_empty());
    }

    #[test]
    fn test_empty_input_returns_empty() {
        assert!(extract_triples("").is_empty());
    }

    #[test]
    fn test_punctuation_on_tokens_does_not_break_match() {
        let triples = extract_triples("Bob is Alice's brother.");
        assert_eq!(triples.len(), 1);
    }

    #[test]
    fn test_lowercase_subject_rejected() {
        let triples = extract_triples("bob is alice's brother");
        assert!(triples.is_empty());
    }
}
