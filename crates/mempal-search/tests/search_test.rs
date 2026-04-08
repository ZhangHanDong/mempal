use mempal_core::{
    db::Database,
    types::{Drawer, SourceType},
};
use mempal_embed::Embedder;
use mempal_search::search;
use tempfile::tempdir;

#[derive(Default)]
struct TestEmbedder;

#[async_trait::async_trait]
impl Embedder for TestEmbedder {
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
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
    for (index, byte) in text.bytes().enumerate() {
        embedding[index % 384] += f32::from(byte) / 255.0;
    }
    embedding
}

fn insert_drawer(
    db: &Database,
    id: &str,
    content: &str,
    wing: &str,
    room: Option<&str>,
    source_file: &str,
) {
    db.insert_drawer(&Drawer {
        id: id.to_string(),
        content: content.to_string(),
        wing: wing.to_string(),
        room: room.map(ToOwned::to_owned),
        source_file: Some(source_file.to_string()),
        source_type: SourceType::Project,
        added_at: "2026-04-08".to_string(),
        chunk_index: Some(0),
    })
    .expect("drawer insert should succeed");

    let vector_json =
        serde_json::to_string(&fake_embedding(content)).expect("vector JSON should serialize");
    db.conn()
        .execute(
            "INSERT INTO drawer_vectors (id, embedding) VALUES (?1, vec_f32(?2))",
            (id, vector_json.as_str()),
        )
        .expect("vector insert should succeed");
}

fn insert_taxonomy(db: &Database, wing: &str, room: &str, keywords: &[&str]) {
    let keywords = serde_json::to_string(keywords).expect("keywords JSON should serialize");
    db.conn()
        .execute(
            "INSERT INTO taxonomy (wing, room, display_name, keywords) VALUES (?1, ?2, ?3, ?4)",
            (wing, room, room, keywords.as_str()),
        )
        .expect("taxonomy insert should succeed");
}

#[tokio::test]
async fn test_search_basic() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    insert_drawer(
        &db,
        "d1",
        "We made the auth decision to use Clerk for this project.",
        "wing_a",
        Some("auth"),
        "/tmp/a.rs",
    );
    insert_drawer(
        &db,
        "d2",
        "Deployment decision notes for Render.",
        "wing_a",
        Some("deploy"),
        "/tmp/b.rs",
    );

    let results = search(&db, &embedder, "auth decision", None, None, 5)
        .await
        .expect("search should succeed");

    assert!(!results.is_empty());
    assert!(results.len() <= 5);
    assert!(!results[0].drawer_id.is_empty());
    assert!(results.iter().all(|result| result.source_file.is_some()));
    assert!(
        results
            .windows(2)
            .all(|pair| pair[0].similarity >= pair[1].similarity)
    );
}

#[tokio::test]
async fn test_search_wing_filter() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    insert_drawer(
        &db,
        "d1",
        "auth decision for wing a",
        "wing_a",
        None,
        "/tmp/a.rs",
    );
    insert_drawer(
        &db,
        "d2",
        "auth decision for wing b",
        "wing_b",
        None,
        "/tmp/b.rs",
    );

    let results = search(&db, &embedder, "auth decision", Some("wing_a"), None, 10)
        .await
        .expect("search should succeed");

    assert!(!results.is_empty());
    assert!(results.iter().all(|result| result.wing == "wing_a"));
}

#[tokio::test]
async fn test_search_wing_room_filter() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    insert_drawer(
        &db,
        "d1",
        "auth integration decision",
        "wing_a",
        Some("room_auth"),
        "/tmp/auth.rs",
    );
    insert_drawer(
        &db,
        "d2",
        "deploy checklist",
        "wing_a",
        Some("room_deploy"),
        "/tmp/deploy.rs",
    );

    let results = search(
        &db,
        &embedder,
        "integration decision",
        Some("wing_a"),
        Some("room_auth"),
        10,
    )
    .await
    .expect("search should succeed");

    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .all(|result| result.wing == "wing_a" && result.room.as_deref() == Some("room_auth"))
    );
}

#[tokio::test]
async fn test_search_citation() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    insert_drawer(
        &db,
        "d1",
        "database decision",
        "wing_a",
        None,
        "/path/to/file.py",
    );

    let results = search(&db, &embedder, "database decision", None, None, 10)
        .await
        .expect("search should succeed");

    assert_eq!(results[0].source_file.as_deref(), Some("/path/to/file.py"));
    assert!(!results[0].drawer_id.is_empty());
    assert!(results[0].route.reason.contains("global"));
}

#[tokio::test]
async fn test_search_empty_db() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    let results = search(&db, &embedder, "anything", None, None, 10)
        .await
        .expect("empty search should succeed");

    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_routes_from_taxonomy() {
    let dir = tempdir().expect("temp dir should be created");
    let db = Database::open(&dir.path().join("test.db")).expect("database should open");
    let embedder = TestEmbedder;

    insert_taxonomy(&db, "myapp", "auth", &["auth", "login", "clerk"]);
    insert_drawer(
        &db,
        "d1",
        "We switched to Clerk because login customization was easier.",
        "myapp",
        Some("auth"),
        "/tmp/auth.md",
    );
    insert_drawer(
        &db,
        "d2",
        "Deployment notes for fly.io.",
        "myapp",
        Some("deploy"),
        "/tmp/deploy.md",
    );

    let results = search(&db, &embedder, "why did we switch to clerk", None, None, 10)
        .await
        .expect("search should succeed");

    assert!(!results.is_empty());
    assert!(results.iter().all(|result| result.room.as_deref() == Some("auth")));
    assert_eq!(results[0].route.wing.as_deref(), Some("myapp"));
    assert_eq!(results[0].route.room.as_deref(), Some("auth"));
    assert!(results[0].route.confidence >= 0.5);
}
