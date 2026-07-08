//! Integration tests for the CLI query subcommands that give the CLI feature
//! parity with the MCP server's query tools (backlog 046-F).
//!
//! These tests exercise the real compiled binary against a temporary workspace
//! with a seeded, `--no-embed` database.  They assert both the human-readable
//! markdown output and the `--json` JSON-RPC envelope shape.  The semantic path
//! is validated only for its no-model failure behaviour so the suite never
//! touches the network or downloads a model.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Helper: path to the compiled binary.
fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

/// Build a docline v1 conformant markdown string for a fixture document.
fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

/// A bogus local model directory that forces the embedding resolver to fail
/// fast **offline** (no Hub download) so semantic/research tests are hermetic.
fn bogus_model_dir(ws: &Path) -> std::path::PathBuf {
    ws.join("no-such-model-dir")
}

/// Seed a workspace with two linked markdown docs and a single local source,
/// run `sync --no-embed`, and return the tempdir handle.
fn seed_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let docs = ws.join("docs");
    std::fs::create_dir_all(&docs).expect("create docs dir");

    // alpha.md carries a distinctive keyword and links to beta.md so the graph
    // has at least one intra-source reference for traversal.
    let alpha = docline_md(
        "alpha.md",
        "Alpha",
        "# Alpha\n\nThe zebrafish genome is documented here. See [Beta](beta.md).\n",
    );
    let beta = docline_md(
        "beta.md",
        "Beta",
        "# Beta\n\nThe kangaroo migration notes live here.\n",
    );
    std::fs::write(docs.join("alpha.md"), alpha).expect("write alpha");
    std::fs::write(docs.join("beta.md"), beta).expect("write beta");

    let config_dir = ws.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        "sources:\n  - type: local\n    id: docs\n    path: docs\n    include:\n      - \"**/*.md\"\n",
    )
    .expect("write sources.yaml");

    let sync_output = Command::new(graphtor_bin())
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

    workspace
}

/// Run a query subcommand in the given workspace and return (success, stdout,
/// stderr).  A bogus model dir is always exported so no invocation can reach
/// the network.
fn run_query(ws: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(graphtor_bin())
        .current_dir(ws)
        .env("GRAPHTOR_EMBED_MODEL_DIR", bogus_model_dir(ws))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run graphtor-docs {args:?}: {e}"));
    (
        output.status.success(),
        String::from_utf8(output.stdout).expect("stdout utf-8"),
        String::from_utf8(output.stderr).expect("stderr utf-8"),
    )
}

#[test]
fn search_human_and_json_return_seeded_chunk() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    // Human output.
    let (ok, stdout, stderr) = run_query(ws, &["search", "zebrafish"]);
    assert!(ok, "search failed: {stderr}");
    assert!(
        stdout.contains("Search Results") && stdout.contains("zebrafish"),
        "human search stdout: {stdout}"
    );

    // JSON output.
    let (ok, stdout, stderr) = run_query(ws, &["--json", "search", "zebrafish"]);
    assert!(ok, "search --json failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("search --json valid json");
    assert_eq!(parsed["jsonrpc"], "2.0");
    let results = parsed["result"]["results"]
        .as_array()
        .expect("result.results is array");
    assert!(!results.is_empty(), "expected >=1 result: {stdout}");
    let first = &results[0];
    assert!(first["chunk_id"].as_str().is_some(), "chunk_id present");
    assert_eq!(first["path"], "alpha.md", "result path: {stdout}");
}

#[test]
fn search_top_k_is_clamped_to_max() {
    // Seed more matching docs than the clamp, then request an absurd --top-k.
    // The CLI must not exceed the shared MAX_SEARCH_TOP_K — the same upper bound
    // the MCP tools enforce — proving the clamp is centralized in `query::*`.
    let doc_count = graphtor_core::query::MAX_SEARCH_TOP_K + 10;

    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();
    let docs = ws.join("docs");
    std::fs::create_dir_all(&docs).expect("create docs dir");
    for i in 0..doc_count {
        let name = format!("clamp-{i}.md");
        let body = format!("# Doc {i}\n\nThe clampkeyword appears in document {i}.\n");
        std::fs::write(
            docs.join(&name),
            docline_md(&name, &format!("Doc {i}"), &body),
        )
        .expect("write clamp doc");
    }
    let config_dir = ws.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        "sources:\n  - type: local\n    id: docs\n    path: docs\n    include:\n      - \"**/*.md\"\n",
    )
    .expect("write sources.yaml");

    let sync = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");
    assert!(
        sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );

    let (ok, stdout, stderr) = run_query(
        ws,
        &["--json", "search", "clampkeyword", "--top-k", "100000"],
    );
    assert!(ok, "search --top-k failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let results = parsed["result"]["results"]
        .as_array()
        .expect("result.results array");
    assert_eq!(
        results.len(),
        graphtor_core::query::MAX_SEARCH_TOP_K,
        "over-large CLI --top-k must be clamped to MAX_SEARCH_TOP_K, got {}",
        results.len()
    );
}

#[test]
fn search_no_match_exits_zero_with_empty_results() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    // Human mode: clear no-results message, exit 0.
    let (ok, stdout, _stderr) = run_query(ws, &["search", "quokkaphotosynthesis"]);
    assert!(ok, "empty search must exit 0");
    assert!(
        stdout.contains("No results found"),
        "human empty stdout: {stdout}"
    );

    // JSON mode: empty results array, exit 0.
    let (ok, stdout, _stderr) = run_query(ws, &["--json", "search", "quokkaphotosynthesis"]);
    assert!(ok, "empty search --json must exit 0");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(
        parsed["result"]["results"].as_array().map(Vec::len),
        Some(0),
        "expected empty results array: {stdout}"
    );
}

#[test]
fn list_sources_human_and_json() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    let (ok, stdout, stderr) = run_query(ws, &["list-sources"]);
    assert!(ok, "list-sources failed: {stderr}");
    assert!(
        stdout.contains("Documentation Sources") && stdout.contains("docs"),
        "human list-sources: {stdout}"
    );

    let (ok, stdout, stderr) = run_query(ws, &["--json", "list-sources"]);
    assert!(ok, "list-sources --json failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let sources = parsed["result"]["sources"]
        .as_array()
        .expect("result.sources array");
    assert_eq!(sources.len(), 1, "expected one source: {stdout}");
    assert_eq!(sources[0]["id"], "docs", "source id: {stdout}");
}

#[test]
fn get_chunk_found_and_not_found() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    // Discover a real chunk id via search --json.
    let (_ok, stdout, _stderr) = run_query(ws, &["--json", "search", "zebrafish"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let chunk_id = parsed["result"]["results"][0]["chunk_id"]
        .as_str()
        .expect("chunk_id string")
        .to_string();

    // Found: human + json.
    let (ok, stdout, stderr) = run_query(ws, &["get-chunk", &chunk_id]);
    assert!(ok, "get-chunk failed: {stderr}");
    assert!(
        stdout.contains(&chunk_id) && stdout.contains("zebrafish"),
        "human get-chunk: {stdout}"
    );

    let (ok, stdout, stderr) = run_query(ws, &["--json", "get-chunk", &chunk_id]);
    assert!(ok, "get-chunk --json failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(parsed["result"]["chunk"]["chunk_id"], chunk_id.as_str());

    // Not found: exit 0, human message, json null.
    let (ok, stdout, _stderr) = run_query(ws, &["get-chunk", "deadbeef-not-a-real-chunk"]);
    assert!(ok, "get-chunk not-found must exit 0");
    assert!(stdout.contains("not found"), "human not-found: {stdout}");

    let (ok, stdout, _stderr) =
        run_query(ws, &["--json", "get-chunk", "deadbeef-not-a-real-chunk"]);
    assert!(ok, "get-chunk --json not-found must exit 0");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(
        parsed["result"]["chunk"],
        Value::Null,
        "not-found chunk should be null: {stdout}"
    );
}

#[test]
fn get_document_human_and_json() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    let (ok, stdout, stderr) = run_query(ws, &["get-document", "alpha.md"]);
    assert!(ok, "get-document failed: {stderr}");
    assert!(
        stdout.contains("Document: `alpha.md`"),
        "human get-document: {stdout}"
    );

    let (ok, stdout, stderr) = run_query(ws, &["--json", "get-document", "alpha.md"]);
    assert!(ok, "get-document --json failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(parsed["result"]["path"], "alpha.md");
    let chunks = parsed["result"]["chunks"]
        .as_array()
        .expect("result.chunks array");
    assert!(!chunks.is_empty(), "expected >=1 chunk: {stdout}");

    // Missing document exits 0 with a clear message.
    let (ok, stdout, _stderr) = run_query(ws, &["get-document", "does-not-exist.md"]);
    assert!(ok, "missing get-document must exit 0");
    assert!(
        stdout.contains("No chunks found"),
        "missing get-document: {stdout}"
    );
}

#[test]
fn traverse_from_seeded_chunk() {
    let workspace = seed_workspace();
    let ws = workspace.path();

    let (_ok, stdout, _stderr) = run_query(ws, &["--json", "search", "zebrafish"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let chunk_id = parsed["result"]["results"][0]["chunk_id"]
        .as_str()
        .expect("chunk_id string")
        .to_string();

    let (ok, stdout, stderr) = run_query(ws, &["--json", "traverse", &chunk_id]);
    assert!(ok, "traverse --json failed: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(parsed["result"]["start_chunk_id"], chunk_id.as_str());
    assert!(
        parsed["result"]["related"].is_array(),
        "related must be an array: {stdout}"
    );
}

#[test]
fn research_degrades_to_keyword_without_model() {
    // No model is available (bogus GRAPHTOR_EMBED_MODEL_DIR), so `research` must
    // degrade to keyword search rather than failing.
    let workspace = seed_workspace();
    let ws = workspace.path();

    let (ok, stdout, stderr) = run_query(ws, &["--json", "research", "zebrafish"]);
    assert!(ok, "research should degrade, not fail: {stderr}");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(parsed["result"]["query"], "zebrafish");
    assert!(
        parsed["result"]["results"].is_array() && parsed["result"]["related"].is_array(),
        "research json shape: {stdout}"
    );
}

#[test]
fn search_semantic_fails_fast_without_model() {
    // Semantic search's entire purpose is embeddings, so a missing model is
    // fatal.  With a bogus model dir the resolver fails offline and the command
    // must exit non-zero with a message that mentions the embedding model.
    let workspace = seed_workspace();
    let ws = workspace.path();

    // Human mode: non-zero exit, stderr explains the missing model.
    let (ok, _stdout, stderr) = run_query(ws, &["search-semantic", "zebrafish"]);
    assert!(!ok, "search-semantic without a model must fail");
    assert!(
        stderr.contains("embedding model"),
        "stderr should mention the embedding model: {stderr}"
    );

    // JSON mode: JSON-RPC error envelope with the same explanation.
    let (ok, stdout, _stderr) = run_query(ws, &["--json", "search-semantic", "zebrafish"]);
    assert!(!ok, "search-semantic --json without a model must fail");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json error envelope");
    let message = parsed["error"]["message"]
        .as_str()
        .expect("error.message string");
    assert!(
        message.contains("embedding model"),
        "error message should mention the embedding model: {message}"
    );
}
