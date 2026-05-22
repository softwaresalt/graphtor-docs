//! Integration tests for multi-database `graphtor-docs status`.

use std::process::Command;

#[test]
fn status_lists_sources_from_multiple_databases() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let src_a = ws.join("docs_a");
    let src_b = ws.join("docs_b");
    std::fs::create_dir_all(&src_a).expect("create docs_a");
    std::fs::create_dir_all(&src_b).expect("create docs_b");
    std::fs::write(src_a.join("a.md"), "# A\n\nContent A.\n").expect("write a");
    std::fs::write(src_b.join("b.md"), "# B\n\nContent B.\n").expect("write b");

    let graphtor_dir = ws.join(".graphtor");
    let config_dir = graphtor_dir.join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let sources_yaml = format!(
        r#"sources:
  - type: local
    id: docs-a
    path: {src_a}
    database: "primary.db"
  - type: local
    id: docs-b
    path: {src_b}
    database: "secondary.db"
"#,
        src_a = src_a.display(),
        src_b = src_b.display(),
    );
    std::fs::write(config_dir.join("sources.yaml"), sources_yaml).expect("write sources.yaml");

    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));

    let sync_output = Command::new(&exe)
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");
    assert!(
        sync_output.status.success(),
        "sync failed: status={:?}\nstderr={}\nstdout={}",
        sync_output.status.code(),
        String::from_utf8_lossy(&sync_output.stderr),
        String::from_utf8_lossy(&sync_output.stdout),
    );

    let status_output = Command::new(&exe)
        .current_dir(ws)
        .arg("status")
        .output()
        .expect("run graphtor-docs status");
    assert!(
        status_output.status.success(),
        "status failed: status={:?}\nstderr={}\nstdout={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stderr),
        String::from_utf8_lossy(&status_output.stdout),
    );

    let stdout = String::from_utf8(status_output.stdout).expect("status stdout utf-8");
    assert!(stdout.contains("primary.db"), "stdout: {stdout}");
    assert!(stdout.contains("secondary.db"), "stdout: {stdout}");
    assert!(stdout.contains("docs-a"), "stdout: {stdout}");
    assert!(stdout.contains("docs-b"), "stdout: {stdout}");
}
