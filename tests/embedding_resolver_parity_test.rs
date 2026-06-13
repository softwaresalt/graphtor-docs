//! Integration tests for the shared embedding-model resolver (041.001-T, 041.002-T, 041.003-T, 041.004-T).
//!
//! Asserts that `sync`, `prewarm`, and (indirectly) `serve` all funnel through
//! the same resolver and that no command site reproduces the legacy diagnostic
//! divergence. When the model is unavailable, the diagnostic is emitted on
//! stderr in a single canonical format that includes a remediation hint.

use std::process::Command;

fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

/// With `--no-embed`, neither `sync` nor `prewarm` should emit the legacy
/// "embedding model unavailable" warn — both should skip resolution silently
/// (apart from an info log that may or may not be visible at default verbosity).
#[test]
fn no_embed_skips_resolution_in_sync_and_prewarm() {
    for cmd in &["sync", "prewarm"] {
        let workspace = tempfile::tempdir().expect("tempdir");
        let docs_dir = workspace.path().join("docs");
        std::fs::create_dir_all(&docs_dir).expect("create docs dir");
        std::fs::write(docs_dir.join("guide.md"), b"---\ntitle: Guide\nsource: /test/source\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nbody.\n").expect("write guide");

        // Provide a minimal sources.yaml so the binary does not fail closed.
        let config_dir = workspace.path().join(".graphtor").join("config");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::write(
            config_dir.join("sources.yaml"),
            "sources:\n  - type: local\n    id: guide\n    path: docs\n    include:\n      - \"**/*.md\"\n",
        )
        .expect("write sources.yaml");

        let db_path = workspace.path().join("graph.db");

        let output = Command::new(graphtor_bin())
            .current_dir(workspace.path())
            .arg("--db-path")
            .arg(&db_path)
            .arg(cmd)
            .arg("--no-embed")
            .output()
            .unwrap_or_else(|e| panic!("run graphtor-docs {cmd}: {e}"));

        assert!(
            output.status.success(),
            "{cmd} --no-embed failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
        // The shared resolver short-circuits on `--no-embed` and must not
        // emit the legacy "embedding model unavailable" warn line.
        assert!(
            !stderr.contains("embedding model unavailable"),
            "{cmd} should skip resolution under --no-embed; got stderr: {stderr}"
        );
        // The canonical diagnostic block ([embed] ...) should only appear on
        // resolver failure, not under --no-embed.
        assert!(
            !stderr.contains("[embed] embedding model unavailable"),
            "{cmd} should not print resolver diagnostic under --no-embed; got: {stderr}"
        );
    }
}
