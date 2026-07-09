use std::collections::BTreeSet;
use std::fmt::Write;

#[cfg(test)]
use super::codec::FLAG_SIGNALS;
use super::codec::{AaakCodec, EMOTION_SIGNALS};
use super::model::AaakMeta;
use super::parse::ALLOWED_FLAGS;

/// Generate the AAAK spec dynamically from code constants.
///
/// Nothing is hardcoded — emotion codes, flags, and the example are all
/// derived from the same tables the codec uses at runtime.
pub fn generate_spec() -> String {
    let emotions = unique_emotion_codes();
    let flags = ALLOWED_FLAGS.join(", ");
    let example = live_example();

    let mut spec = String::with_capacity(1024);
    let _ = write!(
        spec,
        "\
AAAK: compressed memory format (V1). Readable by any LLM without decoding.

FORMAT:
  Header: V{{version}}|{{wing}}|{{room}}|{{date}}|{{source}}
  Zettel: {{id}}:{{ENTITIES}}|{{topics}}|\"{{quote}}\"|{{stars}}|{{emotions}}|{{FLAGS}}
  Tunnel: T:{{left}}<->{{right}}|{{label}}
  Arc:    ARC:{{emo1}}->{{emo2}}->{{emo3}}

ENTITIES: 3-letter uppercase codes (KAI=Kai, CLK=Clerk).
STARS: \u{2605} to \u{2605}\u{2605}\u{2605}\u{2605}\u{2605} (1-5 importance).
EMOTIONS: {emotions}
FLAGS: {flags}

EXAMPLE:
{example}

Read naturally: expand codes, stars=importance, emotions=mindset, flags=category."
    );
    spec
}

fn unique_emotion_codes() -> String {
    let mut seen = BTreeSet::new();
    let mut codes = Vec::new();

    for (_, code) in EMOTION_SIGNALS {
        if seen.insert(*code) {
            codes.push(*code);
        }
    }

    codes.join(", ")
}

fn live_example() -> String {
    let codec = AaakCodec::default();
    let output = codec.encode(
        "Kai recommended Clerk over Auth0 based on pricing and DX.",
        &AaakMeta {
            wing: "myapp".to_string(),
            room: "auth".to_string(),
            date: "2026-04-08".to_string(),
            source: "readme".to_string(),
        },
    );
    output.document.to_string()
}

#[cfg(test)]
fn unique_flag_names() -> String {
    let mut seen = BTreeSet::new();
    let mut flags = Vec::new();

    for (_, flag) in FLAG_SIGNALS {
        if seen.insert(*flag) {
            flags.push(*flag);
        }
    }

    flags.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aaak::AaakDocument;

    #[test]
    fn test_generate_spec_contains_all_flags() {
        let spec = generate_spec();
        for flag in ALLOWED_FLAGS {
            assert!(spec.contains(flag), "spec should contain flag {flag}");
        }
    }

    #[test]
    fn test_generate_spec_contains_live_example() {
        let spec = generate_spec();
        assert!(spec.contains("V1|myapp|auth|"));
        // Extract just the AAAK lines (between EXAMPLE: and the next blank line)
        let example_start = spec.find("V1|").expect("should contain example");
        let example_end = spec[example_start..]
            .find("\n\n")
            .map(|pos| example_start + pos)
            .unwrap_or(spec.len());
        let example = &spec[example_start..example_end];
        AaakDocument::parse(example).expect("embedded example should be valid AAAK");
    }

    #[test]
    fn test_generate_spec_contains_emotion_codes() {
        let spec = generate_spec();
        assert!(spec.contains("determ"));
        assert!(spec.contains("anx"));
        assert!(spec.contains("joy"));
    }

    #[test]
    fn test_unique_flag_names_matches_allowed_flags() {
        let dynamic = unique_flag_names();
        for flag in ALLOWED_FLAGS {
            assert!(
                dynamic.contains(flag),
                "dynamic flags should contain {flag}"
            );
        }
    }
}
