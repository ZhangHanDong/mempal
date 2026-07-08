#![warn(clippy::all)]

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FieldTaxonomyEntry {
    pub field: &'static str,
    pub domains: &'static [&'static str],
    pub description: &'static str,
    pub examples: &'static [&'static str],
}

pub fn field_taxonomy() -> Vec<FieldTaxonomyEntry> {
    FIELD_TAXONOMY.to_vec()
}

const FIELD_TAXONOMY: &[FieldTaxonomyEntry] = &[
    FieldTaxonomyEntry {
        field: "general",
        domains: &["project", "agent", "skill", "global"],
        description: "Default fallback when no narrower field is known.",
        examples: &["project decision", "miscellaneous operational note"],
    },
    FieldTaxonomyEntry {
        field: "epistemics",
        domains: &["global"],
        description: "Cross-domain reasoning rules about evidence, uncertainty, and belief updates.",
        examples: &[
            "evidence precedes assertion",
            "distinguish observation from inference",
        ],
    },
    FieldTaxonomyEntry {
        field: "software-engineering",
        domains: &["project", "skill"],
        description: "General software construction principles and project engineering constraints.",
        examples: &[
            "prefer executable feedback",
            "avoid changing unrelated behavior",
        ],
    },
    FieldTaxonomyEntry {
        field: "debugging",
        domains: &["project", "skill"],
        description: "Fault isolation, reproduction, diagnostics, and verification workflows.",
        examples: &[
            "reproduce before patching",
            "verify the specific failure path",
        ],
    },
    FieldTaxonomyEntry {
        field: "tooling",
        domains: &["project", "agent", "skill"],
        description: "Concrete tool behavior, CLI usage, environment constraints, and version notes.",
        examples: &["cargo clippy invocation", "MCP client startup behavior"],
    },
    FieldTaxonomyEntry {
        field: "research",
        domains: &["project", "agent", "global"],
        description: "External information gathering, source evaluation, and evidence organization.",
        examples: &["research-rs workflow", "source-backed literature summary"],
    },
    FieldTaxonomyEntry {
        field: "writing",
        domains: &["project", "skill"],
        description: "Technical writing, documentation structure, and explanation style.",
        examples: &["design doc structure", "concise PR summary"],
    },
    FieldTaxonomyEntry {
        field: "diary",
        domains: &["agent"],
        description: "Agent diary rollups and session-level behavior memory.",
        examples: &["daily agent rollup", "session handoff note"],
    },
];
