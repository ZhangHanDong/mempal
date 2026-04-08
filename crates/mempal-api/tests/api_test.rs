#[cfg(feature = "rest")]
mod rest_tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use mempal_api::{ApiState, EmbedderFactory, router};
    use mempal_core::{
        db::Database,
        types::{Drawer, SourceType},
    };
    use mempal_embed::Embedder;
    use serde_json::Value;
    use tempfile::tempdir;
    use tower::ServiceExt;

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

    #[derive(Clone, Default)]
    struct TestEmbedderFactory;

    #[async_trait::async_trait]
    impl EmbedderFactory for TestEmbedderFactory {
        async fn build(&self) -> anyhow::Result<Box<dyn Embedder>> {
            Ok(Box::new(TestEmbedder))
        }
    }

    fn fake_embedding(text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0_f32; 384];
        for (index, byte) in text.bytes().enumerate() {
            embedding[index % 384] += f32::from(byte) / 255.0;
        }
        embedding
    }

    fn open_db(path: &Path) -> Database {
        Database::open(path).expect("database should open")
    }

    fn insert_drawer(db: &Database, id: &str, content: &str, wing: &str, room: Option<&str>) {
        db.insert_drawer(&Drawer {
            id: id.to_string(),
            content: content.to_string(),
            wing: wing.to_string(),
            room: room.map(ToOwned::to_owned),
            source_file: Some(format!("/tmp/{id}.md")),
            source_type: SourceType::Project,
            added_at: "1712640000".to_string(),
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

    fn app(db_path: PathBuf) -> axum::Router {
        router(ApiState::new(
            db_path,
            Arc::new(TestEmbedderFactory),
        ))
    }

    #[tokio::test]
    async fn test_api_search() {
        let dir = tempdir().expect("temp dir should exist");
        let db_path = dir.path().join("palace.db");
        let db = open_db(&db_path);
        insert_drawer(
            &db,
            "drawer_auth",
            "We chose Clerk for the auth decision.",
            "myapp",
            Some("auth"),
        );

        let response = app(db_path)
            .oneshot(
                Request::builder()
                    .uri("/api/search?q=auth%20decision")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert!(payload.as_array().and_then(|items| items.first()).is_some());
    }

    #[tokio::test]
    async fn test_api_ingest() {
        let dir = tempdir().expect("temp dir should exist");
        let db_path = dir.path().join("palace.db");

        let response = app(db_path.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/ingest")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "content": "decided to use Clerk",
                            "wing": "myapp",
                            "room": "auth",
                            "source": "/tmp/decision.md"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert!(payload.get("drawer_id").is_some());
    }

    #[tokio::test]
    async fn test_api_taxonomy() {
        let dir = tempdir().expect("temp dir should exist");
        let db_path = dir.path().join("palace.db");
        let db = open_db(&db_path);
        db.conn()
            .execute(
                "INSERT INTO taxonomy (wing, room, display_name, keywords) VALUES (?1, ?2, ?3, ?4)",
                ("myapp", "auth", "auth", "[\"auth\",\"login\"]"),
            )
            .unwrap();

        let response = app(db_path)
            .oneshot(
                Request::builder()
                    .uri("/api/taxonomy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn test_api_status() {
        let dir = tempdir().expect("temp dir should exist");
        let db_path = dir.path().join("palace.db");
        let db = open_db(&db_path);
        insert_drawer(&db, "drawer_auth", "auth decision", "myapp", Some("auth"));

        let response = app(db_path)
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.get("drawer_count").and_then(Value::as_i64), Some(1));
    }
}

#[cfg(not(feature = "rest"))]
#[test]
fn test_no_rest_feature() {
    assert!(!cfg!(feature = "rest"));
}
