use mempal_core::types::TaxonomyEntry;
use mempal_search::route::route_query;

#[test]
fn test_route_hit() {
    let taxonomy = vec![TaxonomyEntry {
        wing: "myapp".to_string(),
        room: "auth".to_string(),
        display_name: Some("Authentication".to_string()),
        keywords: vec![
            "auth".to_string(),
            "login".to_string(),
            "clerk".to_string(),
        ],
    }];

    let decision = route_query("why did we switch to Clerk", &taxonomy);

    assert_eq!(decision.wing.as_deref(), Some("myapp"));
    assert_eq!(decision.room.as_deref(), Some("auth"));
    assert!(decision.confidence >= 0.5);
}

#[test]
fn test_route_fallback() {
    let taxonomy = vec![TaxonomyEntry {
        wing: "myapp".to_string(),
        room: "auth".to_string(),
        display_name: Some("Authentication".to_string()),
        keywords: vec!["auth".to_string(), "login".to_string()],
    }];

    let decision = route_query("what is the weather", &taxonomy);

    assert!(decision.confidence < 0.5);
    assert!(decision.wing.is_none());
    assert!(decision.room.is_none());
}

#[test]
fn test_route_explainable() {
    let taxonomy = vec![TaxonomyEntry {
        wing: "myapp".to_string(),
        room: "auth".to_string(),
        display_name: Some("Authentication".to_string()),
        keywords: vec!["auth".to_string(), "clerk".to_string()],
    }];

    let decision = route_query("clerk auth migration", &taxonomy);

    assert!(!decision.reason.is_empty());
    assert!(decision.reason.contains("clerk") || decision.reason.contains("auth"));
}
