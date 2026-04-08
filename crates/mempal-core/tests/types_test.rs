use mempal_core::types::*;

#[test]
fn test_drawer_fields() {
    let drawer = Drawer {
        id: "drawer_myapp_auth_abc12345".into(),
        content: "decided to use Clerk".into(),
        wing: "myapp".into(),
        room: Some("auth".into()),
        source_file: Some("/path/to/file.py".into()),
        source_type: SourceType::Project,
        added_at: "2026-04-08T12:00:00Z".into(),
        chunk_index: Some(0),
    };

    assert_eq!(drawer.wing, "myapp");
    assert!(drawer.room.is_some());
}

#[test]
fn test_search_result_has_citation() {
    let result = SearchResult {
        drawer_id: "d1".into(),
        content: "test".into(),
        wing: "w".into(),
        room: None,
        source_file: Some("/a.rs".into()),
        similarity: 0.95,
        route: RouteDecision {
            wing: Some("w".into()),
            room: None,
            confidence: 1.0,
            reason: "explicit filters provided: w".into(),
        },
    };

    assert!(result.source_file.is_some());
    assert!(!result.drawer_id.is_empty());
}
