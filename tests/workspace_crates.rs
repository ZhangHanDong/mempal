use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use mempal_agent_memory::{
    anchor::{DEFAULT_FIELD, LEGACY_REPO_ANCHOR_ID},
    types::{AnchorKind, MemoryKind, SourceType},
};
use mempal_embed::{Embedder, EmbedderFactory};
use mempal_mcp_protocol::MEMORY_PROTOCOL;
use mempal_search_core::{DEFAULT_RRF_K, RankedHit, build_fts_match_query, reciprocal_rank_fusion};
use mempal_store_sqlite as store_sqlite;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_manifest(path: impl AsRef<Path>) -> toml::Value {
    let content = fs::read_to_string(path.as_ref()).expect("read manifest");
    toml::from_str(&content).expect("parse manifest")
}

fn dependency_table<'a>(manifest: &'a toml::Value, name: &str) -> &'a toml::value::Table {
    manifest
        .get("dependencies")
        .and_then(|deps| deps.get(name))
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("dependency {name} must be a table"))
}

#[test]
fn test_workspace_manifest_lists_public_path_version_crates() {
    let root = workspace_root();
    let manifest = read_manifest(root.join("Cargo.toml"));
    let members = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .expect("workspace members")
        .iter()
        .map(|value| value.as_str().expect("member string"))
        .collect::<BTreeSet<_>>();

    for member in [
        ".",
        "crates/mempal-embed",
        "crates/mempal-search-core",
        "crates/mempal-agent-memory",
        "crates/mempal-mcp-protocol",
    ] {
        assert!(
            members.contains(member),
            "missing workspace member {member}"
        );
    }

    let root_version = manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .expect("root package version");

    for (name, path) in [
        ("mempal-embed", "crates/mempal-embed"),
        ("mempal-search-core", "crates/mempal-search-core"),
        ("mempal-agent-memory", "crates/mempal-agent-memory"),
        ("mempal-mcp-protocol", "crates/mempal-mcp-protocol"),
    ] {
        let dep = dependency_table(&manifest, name);
        assert_eq!(dep.get("path").and_then(toml::Value::as_str), Some(path));
        assert_eq!(
            dep.get("version").and_then(toml::Value::as_str),
            Some(root_version)
        );

        let crate_manifest = read_manifest(root.join(path).join("Cargo.toml"));
        assert_eq!(
            crate_manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str),
            Some(name)
        );
        assert!(
            crate_manifest
                .get("package")
                .and_then(|package| package.get("publish"))
                .is_none(),
            "{name} must remain publishable"
        );
    }
}

#[test]
fn test_public_reusable_crates_are_directly_usable() {
    struct StaticEmbedder;

    #[async_trait::async_trait]
    impl Embedder for StaticEmbedder {
        async fn embed(&self, texts: &[&str]) -> mempal_embed::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0]).collect())
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn name(&self) -> &str {
            "static"
        }
    }

    struct StaticFactory;

    #[async_trait::async_trait]
    impl EmbedderFactory for StaticFactory {
        async fn build(&self) -> mempal_embed::Result<Box<dyn Embedder>> {
            Ok(Box::new(StaticEmbedder))
        }
    }

    assert_eq!(StaticEmbedder.dimensions(), 3);
    assert_eq!(
        build_fts_match_query(r#"alpha "beta""#).as_deref(),
        Some(r#""alpha" AND """beta""""#)
    );

    let fused = reciprocal_rank_fusion(
        vec![
            vec![("a".to_string(), "vector-a"), ("b".to_string(), "vector-b")],
            vec![("b".to_string(), "fts-b"), ("c".to_string(), "fts-c")],
        ],
        3,
        DEFAULT_RRF_K,
    );
    assert_eq!(
        fused.iter().map(|hit| hit.key.as_str()).collect::<Vec<_>>(),
        vec!["b", "a", "c"]
    );
    assert!(matches!(
        fused.first(),
        Some(RankedHit {
            item: "vector-b",
            ..
        })
    ));

    assert_eq!(MemoryKind::Evidence, MemoryKind::Evidence);
    assert_eq!(DEFAULT_FIELD, "general");
    assert!(LEGACY_REPO_ANCHOR_ID.starts_with("repo://"));
    assert!(MEMORY_PROTOCOL.contains("MEMPAL MEMORY PROTOCOL"));

    let _factory = StaticFactory;
    let _anchor = AnchorKind::Repo;
    let _source_type = SourceType::Project;
}

#[test]
fn test_mempal_facade_preserves_legacy_public_paths() {
    fn assert_embedder<E: mempal::embed::Embedder>() {}

    struct LegacyEmbedder;

    #[async_trait::async_trait]
    impl mempal::embed::Embedder for LegacyEmbedder {
        async fn embed(&self, texts: &[&str]) -> mempal::embed::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn name(&self) -> &str {
            "legacy"
        }
    }

    assert_embedder::<LegacyEmbedder>();
    assert_eq!(
        mempal::core::types::MemoryKind::Evidence,
        MemoryKind::Evidence
    );
    assert_eq!(mempal::core::anchor::DEFAULT_FIELD, DEFAULT_FIELD);
    assert_eq!(
        mempal::core::protocol::MEMORY_PROTOCOL,
        mempal_mcp_protocol::MEMORY_PROTOCOL
    );
}

#[test]
fn test_workspace_split_does_not_mark_reusable_crates_private_or_change_schema() {
    let root = workspace_root();
    for path in [
        "crates/mempal-embed",
        "crates/mempal-search-core",
        "crates/mempal-agent-memory",
        "crates/mempal-mcp-protocol",
    ] {
        let manifest = fs::read_to_string(root.join(path).join("Cargo.toml"))
            .unwrap_or_else(|error| panic!("read {path}/Cargo.toml: {error}"));
        assert!(
            !manifest.contains("publish = false"),
            "{path} must remain a public publishable crate"
        );
    }

    let db_source = fs::read_to_string(root.join("crates/mempal-store-sqlite/src/lib.rs"))
        .expect("read store lib.rs");
    assert!(
        db_source.contains("pub const CURRENT_SCHEMA_VERSION: u32 = 9;"),
        "workspace split must not change the SQLite schema version"
    );
}

#[test]
fn test_p114_workspace_lists_coarse_runtime_crates() {
    let root = workspace_root();
    let manifest = read_manifest(root.join("Cargo.toml"));
    let members = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .expect("workspace members")
        .iter()
        .map(|value| value.as_str().expect("member string"))
        .collect::<BTreeSet<_>>();

    for member in [
        "crates/mempal-agent-memory",
        "crates/mempal-embed",
        "crates/mempal-mcp-protocol",
        "crates/mempal-search-core",
        "crates/mempal-store-sqlite",
        "crates/mempal-runtime",
        "crates/mempal-mcp-server",
    ] {
        assert!(
            members.contains(member),
            "missing coarse workspace member {member}"
        );
    }

    for forbidden in [
        "crates/mempal-knowledge",
        "crates/mempal-cowork",
        "crates/mempal-context",
        "crates/mempal-factcheck",
    ] {
        assert!(
            !members.contains(forbidden),
            "P114 should not create fine-grained feature crate {forbidden}"
        );
    }
}

#[test]
fn test_p114_store_sqlite_crate_owns_database_schema() {
    fn assert_store_database(_: Option<store_sqlite::Database>) {}
    fn assert_store_error(_: Option<store_sqlite::DbError>) {}

    assert_eq!(store_sqlite::CURRENT_SCHEMA_VERSION, 9);
    assert_store_database(None);
    assert_store_error(None);
}

#[test]
fn test_p114_runtime_and_mcp_crates_are_directly_usable() {
    fn assert_runtime_search_filters(_: mempal_runtime::search::SearchFilters) {}
    fn assert_runtime_context_request(_: Option<mempal_runtime::context::ContextRequest>) {}
    fn assert_mcp_server(_: Option<mempal_mcp_server::MempalMcpServer>) {}

    assert_runtime_search_filters(mempal_runtime::search::SearchFilters::default());
    assert_runtime_context_request(None);
    assert_mcp_server(None);

    let _ingest_options = mempal_runtime::ingest::IngestOptions::default();
}

#[test]
fn test_p114_root_preserves_legacy_facades() {
    fn assert_root_database(_: Option<mempal::core::db::Database>) {}
    fn assert_root_search_filters(_: mempal::search::SearchFilters) {}
    fn assert_root_context_request(_: Option<mempal::context::ContextRequest>) {}
    fn assert_root_mcp_server(_: Option<mempal::mcp::MempalMcpServer>) {}

    assert_eq!(
        mempal::core::db::CURRENT_SCHEMA_VERSION,
        store_sqlite::CURRENT_SCHEMA_VERSION
    );
    assert_root_database(None);
    assert_root_search_filters(mempal::search::SearchFilters::default());
    assert_root_context_request(None);
    assert_root_mcp_server(None);

    let _ingest_options = mempal::ingest::IngestOptions::default();
}

#[test]
fn test_p114_publishable_crates_and_root_package_are_preserved() {
    let root = workspace_root();
    let manifest = read_manifest(root.join("Cargo.toml"));
    let root_package = manifest.get("package").expect("root package");
    let root_version = root_package
        .get("version")
        .and_then(toml::Value::as_str)
        .expect("root package version");

    assert_eq!(
        root_package.get("name").and_then(toml::Value::as_str),
        Some("mempal")
    );

    for (name, path) in [
        ("mempal-store-sqlite", "crates/mempal-store-sqlite"),
        ("mempal-runtime", "crates/mempal-runtime"),
        ("mempal-mcp-server", "crates/mempal-mcp-server"),
    ] {
        let dep = dependency_table(&manifest, name);
        assert_eq!(dep.get("path").and_then(toml::Value::as_str), Some(path));
        assert_eq!(
            dep.get("version").and_then(toml::Value::as_str),
            Some(root_version)
        );

        let crate_manifest = read_manifest(root.join(path).join("Cargo.toml"));
        assert_eq!(
            crate_manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str),
            Some(name)
        );
        assert!(
            crate_manifest
                .get("package")
                .and_then(|package| package.get("publish"))
                .is_none(),
            "{name} must remain publishable"
        );
    }
}
