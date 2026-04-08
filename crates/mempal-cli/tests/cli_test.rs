use std::fs;
use std::path::Path;
use std::process::Command;

use mempal_core::{
    db::Database,
    types::{Drawer, SourceType},
};
use serde_json::Value;
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    fs::write(path, content).expect("fixture file should be written");
}

fn run_cli(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mempal"))
        .env("HOME", home)
        .args(args)
        .output()
        .expect("cli should run")
}

fn seed_db(home: &Path) -> Database {
    let mempal_dir = home.join(".mempal");
    fs::create_dir_all(&mempal_dir).expect("mempal dir should exist");
    Database::open(&mempal_dir.join("palace.db")).expect("database should open")
}

fn insert_drawer(db: &Database, id: &str, wing: &str, room: Option<&str>, content: &str) {
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
}

fn insert_taxonomy(db: &Database, wing: &str, room: &str, keywords: &[&str]) {
    let keywords = serde_json::to_string(keywords).expect("keywords should serialize");
    db.conn()
        .execute(
            "INSERT INTO taxonomy (wing, room, display_name, keywords) VALUES (?1, ?2, ?3, ?4)",
            (wing, room, room, keywords.as_str()),
        )
        .expect("taxonomy insert should succeed");
}

#[test]
fn test_e2e_init_ingest_search() {
    let home = tempdir().expect("home temp dir should be created");
    let project = tempdir().expect("project temp dir should be created");
    let src_auth = project.path().join("src").join("auth");
    fs::create_dir_all(&src_auth).expect("project directories should be created");
    write_file(
        &project.path().join("README.md"),
        "database decision: we decided to use PostgreSQL for analytics.",
    );
    write_file(&src_auth.join("mod.rs"), "pub fn login() {}");

    let init = run_cli(
        home.path(),
        &["init", project.path().to_str().expect("valid path")],
    );
    assert!(init.status.success(), "init failed: {:?}", init);
    let init_stdout = String::from_utf8(init.stdout).expect("stdout should be utf8");
    assert!(init_stdout.contains("auth"));

    let db_path = home.path().join(".mempal").join("palace.db");
    let db = Database::open(&db_path).expect("database should open");
    let taxonomy_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM taxonomy", [], |row| row.get(0))
        .expect("taxonomy count query should succeed");
    assert!(taxonomy_count > 0);

    let ingest = run_cli(
        home.path(),
        &[
            "ingest",
            project.path().to_str().expect("valid path"),
            "--wing",
            "myproject",
        ],
    );
    assert!(ingest.status.success(), "ingest failed: {:?}", ingest);
    let ingest_stdout = String::from_utf8(ingest.stdout).expect("stdout should be utf8");
    assert!(ingest_stdout.contains("chunks"));

    let search = run_cli(
        home.path(),
        &[
            "search",
            "database decision postgresql analytics",
            "--json",
            "--wing",
            "myproject",
        ],
    );
    assert!(search.status.success(), "search failed: {:?}", search);
    let search_stdout = String::from_utf8(search.stdout).expect("stdout should be utf8");
    let results: Value =
        serde_json::from_str(&search_stdout).expect("search output should be JSON");
    let first = results
        .as_array()
        .and_then(|items| items.first())
        .expect("search should return at least one result");
    let source_file = first
        .get("source_file")
        .and_then(Value::as_str)
        .expect("result should include source_file");
    assert!(source_file.ends_with("README.md"));
}

#[test]
fn test_cli_wakeup() {
    let home = tempdir().expect("home temp dir should be created");
    let db = seed_db(home.path());
    insert_drawer(
        &db,
        "drawer_a",
        "myapp",
        Some("auth"),
        "Decision: use Clerk for auth because integration is simpler.",
    );
    insert_drawer(
        &db,
        "drawer_b",
        "myapp",
        Some("deploy"),
        "Deploy on Fly.io after the auth migration is stable.",
    );

    let output = run_cli(home.path(), &["wake-up"]);
    assert!(output.status.success(), "wake-up failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("L0"));
    assert!(stdout.contains("L1"));
    assert!(stdout.contains("estimated_tokens"));
    assert!(stdout.contains("Clerk"));
}

#[test]
fn test_cli_status() {
    let home = tempdir().expect("home temp dir should be created");
    let db = seed_db(home.path());
    insert_taxonomy(&db, "myapp", "auth", &["auth", "login"]);
    insert_drawer(&db, "drawer_a", "myapp", Some("auth"), "Auth decision");
    insert_drawer(&db, "drawer_b", "myapp", Some("deploy"), "Deploy decision");

    let output = run_cli(home.path(), &["status"]);
    assert!(output.status.success(), "status failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("drawer_count"));
    assert!(stdout.contains("db_size_bytes"));
    assert!(stdout.contains("myapp/auth"));
    assert!(stdout.contains("myapp/deploy"));
}

#[test]
fn test_cli_taxonomy_list_and_edit() {
    let home = tempdir().expect("home temp dir should be created");
    let db = seed_db(home.path());
    insert_taxonomy(&db, "myapp", "auth", &["auth", "login"]);

    let list = run_cli(home.path(), &["taxonomy", "list"]);
    assert!(list.status.success(), "taxonomy list failed: {:?}", list);
    let list_stdout = String::from_utf8(list.stdout).expect("stdout should be utf8");
    assert!(list_stdout.contains("myapp/auth"));
    assert!(list_stdout.contains("auth, login"));

    let edit = run_cli(
        home.path(),
        &[
            "taxonomy",
            "edit",
            "myapp",
            "auth",
            "--keywords",
            "auth,login,clerk",
        ],
    );
    assert!(edit.status.success(), "taxonomy edit failed: {:?}", edit);

    let edited_db = seed_db(home.path());
    let keywords: String = edited_db
        .conn()
        .query_row(
            "SELECT keywords FROM taxonomy WHERE wing = ?1 AND room = ?2",
            ("myapp", "auth"),
            |row| row.get(0),
        )
        .expect("taxonomy row should exist");
    assert!(keywords.contains("clerk"));
}
