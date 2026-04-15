//! Integration tests for P8 cowork inbox push.
//!
//! Run with:
//!   cargo test --test cowork_inbox --no-default-features --features model2vec
//!
//! These tests exercise inbox behavior that must hold under real process
//! boundaries (concurrent drain, CLI-level graceful degrade, stdin-json
//! payload parsing). Unit coverage for push/drain/format lives inline in
//! src/cowork/inbox.rs.

use mempal::cowork::Tool;
use mempal::cowork::inbox::{InboxMessage, drain, push};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn setup_repo(tmp: &TempDir, name: &str) -> PathBuf {
    let repo = tmp.path().join(name);
    fs::create_dir_all(repo.join(".git")).unwrap();
    repo
}

#[tokio::test]
async fn concurrent_drain_is_winner_takes_all_at_most_once() {
    let tmp = TempDir::new().unwrap();
    let mempal_home = Arc::new(tmp.path().to_path_buf());
    let repo = setup_repo(&tmp, "proj");
    let repo_arc = Arc::new(repo);

    for i in 0..3 {
        push(
            &mempal_home,
            Tool::Claude,
            Tool::Codex,
            &repo_arc,
            format!("concurrent-{i}"),
            "2026-04-15T02:00:00Z".into(),
        )
        .unwrap();
    }

    let home_a = Arc::clone(&mempal_home);
    let repo_a = Arc::clone(&repo_arc);
    let home_b = Arc::clone(&mempal_home);
    let repo_b = Arc::clone(&repo_arc);

    let task_a =
        tokio::task::spawn_blocking(move || drain(&home_a, Tool::Codex, &repo_a).unwrap());
    let task_b =
        tokio::task::spawn_blocking(move || drain(&home_b, Tool::Codex, &repo_b).unwrap());

    let (a, b) = tokio::join!(task_a, task_b);
    let a_msgs: Vec<InboxMessage> = a.unwrap();
    let b_msgs: Vec<InboxMessage> = b.unwrap();

    // Exactly one task won the whole batch; the other got nothing.
    let total_received = a_msgs.len() + b_msgs.len();
    assert_eq!(
        total_received, 3,
        "both tasks combined must see all 3 messages"
    );

    let winner_count = std::cmp::max(a_msgs.len(), b_msgs.len());
    let loser_count = std::cmp::min(a_msgs.len(), b_msgs.len());
    assert_eq!(winner_count, 3, "winner takes all 3");
    assert_eq!(loser_count, 0, "loser empty");

    // No duplicate delivery.
    let winner_contents: Vec<String> = if a_msgs.len() == 3 {
        a_msgs.iter().map(|m| m.content.clone()).collect()
    } else {
        b_msgs.iter().map(|m| m.content.clone()).collect()
    };
    assert_eq!(
        winner_contents,
        vec!["concurrent-0", "concurrent-1", "concurrent-2"]
    );
}
