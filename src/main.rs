//! `graphtor-docs` — `GraphRAG` documentation index binary entry point.
//!
//! Provides a `clap`-based CLI with the following subcommands:
//!
//! - `sync`      — incremental sync (default) or full pipeline (`--full`)
//! - `serve`     — start the MCP STDIO server (`localhost` only)
//! - `status`    — print database statistics
//! - `init`      — generate a template `sources.yaml`
//! - `install`   — install graphtor-docs into the current workspace
//! - `doctor`    — diagnose workspace health
//! - `upgrade`   — upgrade the installed binary
//! - `uninstall` — remove graphtor-docs from the current workspace
//!
//! # Exit codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | success |
//! | 1    | partial failures (some files failed; others succeeded) |
//! | 2    | fatal error (pipeline stage failed, database unavailable, etc.) |

#![forbid(unsafe_code)]

mod cli;
mod workspace;

use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use clap::Parser as _;
use graphtor_core::mcp::{DocServer, SyncStatus};
use graphtor_core::{
    acquire::{execute as acquire_execute, plan as acquire_plan},
    config::SourceConfig,
    db::{list_sources, DataStore},
    init_logging,
    sync::sync_source,
    EmbeddingModel, LocalSource, LogVerbosity, PipelineConfig, Source,
};
use tracing::{error, info, warn};

use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let verbosity = if cli.verbose {
        LogVerbosity::Verbose
    } else {
        LogVerbosity::Normal
    };

    if let Err(e) = init_logging(verbosity) {
        eprintln!("error: failed to initialize logging: {e}");
        process::exit(2);
    }

    let exit_code = match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            error!(error = %e, "fatal error");
            eprintln!("error: {e}");
            2
        }
    };

    process::exit(exit_code);
}

/// Dispatch the CLI command and return an exit code.
///
/// Returns `0` for success, `1` for partial failures, `2` for fatal errors.
async fn run(cli: Cli) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir().context("failed to determine working directory")?;

    // Resolve database path: CLI flag > env var (already handled by clap env=) > default.
    let db_path: PathBuf = cli
        .db_path
        .clone()
        .unwrap_or_else(|| cwd.join(".graphtor/graph.db"));

    // Warn on deprecated --data-dir alias usage (flag form and =value form).
    if std::env::args().any(|a| a == "--data-dir" || a.starts_with("--data-dir=")) {
        eprintln!("warning: --data-dir is deprecated; use --db-path instead");
    }

    // Extract shared fields before destructuring the command.
    let sources_path = cli.config.clone();
    let verbose = cli.verbose;
    let _ = verbose; // available for future use

    match cli.command {
        Command::Sync(args) => cmd_sync(&cwd, &db_path, sources_path.as_deref(), &args),
        Command::Serve(_) => cmd_serve(&db_path, &cwd, sources_path.as_deref()).await,
        Command::Status(args) => cmd_status(&db_path, &cwd, &args),
        Command::Init(args) => cmd_init(&cwd, &args),
        Command::Install(args) => cmd_install(&cwd, &args),
        Command::Doctor => Ok(cmd_doctor(&cwd)),
        Command::Upgrade(args) => cmd_upgrade(&cwd, &args),
        Command::Uninstall(args) => cmd_uninstall(&cwd, &args),
    }
}

// ── sync ──────────────────────────────────────────────────────────────────────

/// Build a [`SourceConfig`] that indexes all `.md` files in `cwd`.
///
/// Used when no `sources.yaml` is found and no explicit `--config` was
/// passed — the workspace root is treated as an implicit local source named
/// `"workspace"`.  Standard tooling directories (`.graphtor`, `.git`, `target`)
/// are excluded automatically.
fn build_workspace_source_config(cwd: &std::path::Path) -> SourceConfig {
    SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "workspace".to_string(),
            path: cwd.to_path_buf(),
            include: vec!["**/*.md".to_string(), "**/*.markdown".to_string()],
            exclude: vec![
                ".graphtor/**".to_string(),
                ".git/**".to_string(),
                "target/**".to_string(),
            ],
            formats: vec!["md".to_string(), "markdown".to_string()],
        })],
    }
}

/// Resolve a [`SourceConfig`] from an optional config-file override or auto-discovery.
///
/// Resolution order:
/// 1. `config_override` is `Some` and the file exists → load and parse it.
/// 2. `config_override` is `Some` and the file **does not** exist → return `Ok(None)`
///    so the caller can surface an appropriate error.
/// 3. `config_override` is `None` and the default path exists → load it.
/// 4. `config_override` is `None` and the default path is missing → auto-discover
///    the workspace (`build_workspace_source_config`).
///
/// Returns `Err` only when a file exists but cannot be read or parsed (always fatal).
fn load_source_config(
    cwd: &std::path::Path,
    config_override: Option<&std::path::Path>,
) -> anyhow::Result<Option<SourceConfig>> {
    let default_path = cwd.join(".graphtor/config/sources.yaml");
    let path = config_override.map_or(default_path.as_path(), |p| p);

    if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let cfg: SourceConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(cfg))
    } else if config_override.is_some() {
        // Explicit override provided but missing — caller decides how to handle.
        Ok(None)
    } else {
        // Default path missing: fall back to workspace auto-discovery.
        info!("no sources.yaml found; using workspace auto-discovery (indexing .md files)");
        eprintln!(
            "info: no sources.yaml found — indexing .md files in the current directory. \
             Run `graphtor-docs init` to create a sources.yaml."
        );
        Ok(Some(build_workspace_source_config(cwd)))
    }
}

fn cmd_sync(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    config_override: Option<&std::path::Path>,
    args: &cli::SyncArgs,
) -> anyhow::Result<i32> {
    // Resolve source config: explicit override → default path → workspace auto-discovery.
    let source_config: SourceConfig = if let Some(cfg) = load_source_config(cwd, config_override)? {
        cfg
    } else {
        // Only reachable when config_override is Some but the file does not exist.
        let path = config_override.unwrap_or_else(|| std::path::Path::new("(unknown)"));
        eprintln!("error: sources.yaml not found at {}", path.display());
        return Ok(2);
    };

    if source_config.sources.is_empty() {
        warn!("sources.yaml contains no sources; nothing to sync");
        println!(
            "No sources configured. Add documentation sources and re-run `graphtor-docs sync`."
        );
        return Ok(0);
    }

    // Open (or create) the database.
    let store = DataStore::open_sqlite(db_path, cwd)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    store
        .ensure_schema()
        .context("failed to ensure database schema")?;

    // Build acquisition plan.
    let data_root: PathBuf = args
        .data_root
        .clone()
        .unwrap_or_else(|| cwd.join(".graphtor/data"));
    let plan = acquire_plan::plan(&source_config, &data_root, cwd)
        .context("failed to build acquisition plan")?;

    // Optionally load the embedding model.
    let model: Option<EmbeddingModel> = if args.no_embed {
        None
    } else {
        match EmbeddingModel::load("sentence-transformers/all-MiniLM-L6-v2") {
            Ok(m) => Some(m),
            Err(e) => {
                warn!(error = %e, "embedding model unavailable; proceeding without embeddings");
                None
            }
        }
    };

    if args.full {
        cmd_sync_full(&store, &plan, model.as_ref(), args)
    } else {
        Ok(cmd_sync_incremental(db_path, &store, &plan, model.as_ref()))
    }
}

/// Full pipeline: acquire → parse → embed → load all files unconditionally.
fn cmd_sync_full(
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
    args: &cli::SyncArgs,
) -> anyhow::Result<i32> {
    let pipeline_config = PipelineConfig {
        batch_size: args.batch_size,
        parallel: false,
    };

    info!(
        sources = plan.sources.len(),
        batch_size = args.batch_size,
        "starting full sync"
    );

    let result = graphtor_core::pipeline::run(plan, store, model, &pipeline_config)
        .context("pipeline execution failed")?;

    println!(
        "sync complete (full): {} documents, {} chunks",
        result.documents_processed, result.total_chunks
    );

    if result.errors_encountered.is_empty() {
        Ok(0)
    } else {
        eprintln!("{} file(s) failed:", result.errors_encountered.len());
        for fe in &result.errors_encountered {
            eprintln!("  {}: {}", fe.path.display(), fe.error);
        }
        Ok(1)
    }
}

/// Incremental sync: acquire new sources, then detect and re-ingest only changes.
fn cmd_sync_incremental(
    db_path: &std::path::Path,
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
) -> i32 {
    info!(sources = plan.sources.len(), "starting incremental sync");

    // Execute acquisition to clone any new git repos (existing ones are skipped).
    let acq_result = acquire_execute(plan, false);
    if acq_result.failed > 0 {
        warn!(
            failed = acq_result.failed,
            succeeded = acq_result.succeeded,
            "acquisition had failures; affected sources may be skipped"
        );
    }

    // Derive sync state path from the database location so state and DB stay colocated.
    let state_path = db_path.parent().map_or_else(
        || PathBuf::from("sync_state.json"),
        |p| p.join("sync_state.json"),
    );

    let mut total_files: usize = 0;
    let mut total_chunks: usize = 0;
    let mut total_deleted: usize = 0;
    let mut total_errors: usize = acq_result.failed;

    for planned in &plan.sources {
        let source_dir = &planned.target_dir;
        let source_id = match &planned.source {
            graphtor_core::Source::Git(g) => g.id.as_str(),
            graphtor_core::Source::Local(l) => l.id.as_str(),
            graphtor_core::Source::Url(u) => u.id.as_str(),
        };

        if !source_dir.exists() {
            warn!(
                source_id,
                path = %source_dir.display(),
                "source directory does not exist; skipping"
            );
            total_errors += 1;
            continue;
        }

        match sync_source(
            store,
            &planned.source,
            source_dir,
            &state_path,
            &plan.allowed_root,
            model,
        ) {
            Ok(result) => {
                total_files += result.files_processed;
                total_chunks += result.chunks_loaded;
                total_deleted += result.files_deleted;
                total_errors += result.files_errored;
            }
            Err(e) => {
                warn!(
                    source_id,
                    error = %e,
                    "incremental sync failed for source; continuing"
                );
                total_errors += 1;
            }
        }
    }

    println!(
        "sync complete (incremental): {total_files} files processed, {total_chunks} chunks loaded, {total_deleted} files deleted"
    );

    if total_errors == 0 {
        0
    } else {
        eprintln!("{total_errors} error(s) encountered during sync");
        1
    }
}

// ── serve ─────────────────────────────────────────────────────────────────────

/// Spawn a background incremental sync task and return the shared status handle.
///
/// Follows the same high-level flow as `cmd_sync_incremental`: acquire new git
/// repos → sync per source → aggregate errors.  Unlike the interactive command,
/// this function does not track deleted files and reports progress through the
/// returned `Arc<Mutex<SyncStatus>>` rather than writing to stdout.  The returned
/// handle is shared with the `DocServer` so `get_status` can reflect live sync state.
fn spawn_background_sync(
    source_config: SourceConfig,
    db_path_owned: PathBuf,
    cwd_owned: PathBuf,
    store_bg: DataStore,
    model_bg: Option<EmbeddingModel>,
) -> Arc<Mutex<SyncStatus>> {
    let sync_status: Arc<Mutex<SyncStatus>> = Arc::default();
    let sync_status_bg = Arc::clone(&sync_status);

    tokio::spawn(async move {
        {
            let mut guard = sync_status_bg
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = SyncStatus::Syncing;
        }

        let result = tokio::task::spawn_blocking(move || {
            let data_root = cwd_owned.join(".graphtor/data");
            let plan = acquire_plan::plan(&source_config, &data_root, &cwd_owned)
                .context("background sync: failed to build acquisition plan")?;

            // Clone any new git repos; local and URL sources skip acquisition.
            let acq_result = acquire_execute(&plan, false);
            let mut errors = acq_result.failed;
            if errors > 0 {
                warn!(failed = errors, "background sync: acquisition had failures");
            }

            let state_path = db_path_owned.parent().map_or_else(
                || PathBuf::from("sync_state.json"),
                |p| p.join("sync_state.json"),
            );

            let mut files: usize = 0;
            let mut chunks: usize = 0;

            for planned in &plan.sources {
                let source_dir = &planned.target_dir;
                if !source_dir.exists() {
                    warn!(
                        path = %source_dir.display(),
                        "background sync: source directory missing; skipping"
                    );
                    errors += 1;
                    continue;
                }
                match sync_source(
                    &store_bg,
                    &planned.source,
                    source_dir,
                    &state_path,
                    &plan.allowed_root,
                    model_bg.as_ref(),
                ) {
                    Ok(r) => {
                        files += r.files_processed;
                        chunks += r.chunks_loaded;
                        errors += r.files_errored;
                    }
                    Err(e) => {
                        warn!(error = %e, "background sync: source failed");
                        errors += 1;
                    }
                }
            }

            Ok::<(usize, usize, usize), anyhow::Error>((files, chunks, errors))
        })
        .await;

        let new_status = match result {
            Ok(Ok((f, c, 0))) => SyncStatus::Done {
                files: f,
                chunks: c,
            },
            Ok(Ok((_, _, e))) => SyncStatus::Error(format!("{e} source(s) had errors")),
            Ok(Err(e)) => SyncStatus::Error(e.to_string()),
            Err(e) => SyncStatus::Error(format!("task panicked: {e}")),
        };

        let mut guard = sync_status_bg
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = new_status;
    });

    sync_status
}

async fn cmd_serve(
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    config_override: Option<&std::path::Path>,
) -> anyhow::Result<i32> {
    info!(db_path = %db_path.display(), "opening database");
    let store = DataStore::open_sqlite(db_path, cwd)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    store
        .ensure_schema()
        .context("failed to ensure database schema")?;

    // Optionally load the embedding model for semantic search.
    let model: Option<EmbeddingModel> =
        match EmbeddingModel::load("sentence-transformers/all-MiniLM-L6-v2") {
            Ok(m) => {
                info!("embedding model loaded; semantic search enabled");
                Some(m)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "embedding model unavailable; semantic search disabled"
                );
                None
            }
        };

    // Resolve source config (same auto-discovery logic as cmd_sync) and spawn
    // a background incremental sync task if a config is available.
    let sync_status = match load_source_config(cwd, config_override) {
        Ok(Some(source_config)) if !source_config.sources.is_empty() => {
            info!("background sync task spawned");
            spawn_background_sync(
                source_config,
                db_path.to_path_buf(),
                cwd.to_path_buf(),
                store.clone(),
                model.clone(),
            )
        }
        Ok(Some(_)) => {
            info!("source config has no sources; background sync skipped");
            Arc::default()
        }
        Ok(None) => {
            // load_source_config returns Ok(None) only when an explicit --config
            // path was supplied but the file does not exist — fail fast.
            let path = config_override.unwrap_or_else(|| std::path::Path::new("<unknown>"));
            eprintln!("error: config file '{}' not found", path.display());
            return Ok(2);
        }
        Err(e) => {
            warn!(error = %e, "failed to load source config; background sync disabled");
            Arc::default()
        }
    };

    let server = match model {
        Some(m) => DocServer::with_model(store, m),
        None => DocServer::new(store),
    };
    let server = server.with_sync_status(sync_status);

    info!("starting MCP STDIO server");
    rmcp::serve_server(server, rmcp::transport::stdio())
        .await
        .context("MCP server failed to start")?
        .waiting()
        .await
        .context("MCP server terminated with error")?;

    Ok(0)
}

// ── status ────────────────────────────────────────────────────────────────────

fn cmd_status(
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    args: &cli::StatusArgs,
) -> anyhow::Result<i32> {
    if !db_path.exists() {
        println!("database not found — run `graphtor-docs sync` to create it");
        return Ok(0);
    }

    let store = DataStore::open_sqlite(db_path, cwd)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    store.ensure_schema().context("failed to ensure schema")?;

    let sources = list_sources(&store).context("failed to list sources")?;

    if args.json {
        let json = serde_json::json!({
            "database": db_path.display().to_string(),
            "sources": sources.iter().map(|s| serde_json::json!({
                "id": s.source_id,
                "name": s.name,
                "kind": s.kind,
                "url": s.url,
                "synced_at": s.synced_at,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("database: {}", db_path.display());
        println!("sources:  {}", sources.len());
        for s in &sources {
            println!(
                "  [{kind}] {id} — {url} (last sync: {synced})",
                kind = s.kind,
                id = s.source_id,
                url = s.url,
                synced = s.synced_at.as_deref().unwrap_or("never"),
            );
        }
    }

    Ok(0)
}

// ── init ──────────────────────────────────────────────────────────────────────

fn cmd_init(cwd: &std::path::Path, args: &cli::InitArgs) -> anyhow::Result<i32> {
    // Locate or create workspace dir.
    let workspace_dir = cwd.join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).context("failed to create .graphtor/")?;

    let result = workspace::init::init_sources_yaml(&workspace_dir, args.force)
        .context("failed to initialise sources.yaml")?;

    if result.created {
        println!("created: {}", result.path.display());
        println!("edit the file to add your documentation sources, then run `graphtor-docs sync`");
    } else {
        println!("already exists: {}", result.path.display());
        println!("use --force to overwrite");
    }

    Ok(0)
}

// ── install ───────────────────────────────────────────────────────────────────

fn cmd_install(cwd: &std::path::Path, args: &cli::InstallArgs) -> anyhow::Result<i32> {
    // Always create the workspace directory scaffold first so the lock path exists.
    let ws_dir = cwd.join(".graphtor");
    std::fs::create_dir_all(&ws_dir).context("failed to create .graphtor directory")?;

    // Always acquire a lock to prevent concurrent installs.
    let _lock = workspace::lock::WorkspaceLock::acquire(&ws_dir, args.force_unlock)
        .context("workspace is locked by another process")?;

    let result = workspace::install::install(cwd).context("install failed")?;

    if result.created {
        println!("created: {}", result.workspace_dir.display());
    } else {
        println!(
            "workspace already exists: {}",
            result.workspace_dir.display()
        );
    }
    println!("binary:  {}", result.binary_path.display());

    // Initialise sources.yaml (non-destructive).
    let init_result = workspace::init::init_sources_yaml(&result.workspace_dir, false)
        .context("failed to initialise sources.yaml")?;
    if init_result.created {
        println!("created: {}", init_result.path.display());
    }

    // Manage .gitignore.
    if !args.no_gitignore {
        workspace::gitignore::add_gitignore_entry(cwd).context("failed to update .gitignore")?;
        println!("updated: .gitignore (added .graphtor/)");
    }

    // Generate MCP client configs.
    let editors: Vec<workspace::mcp_config::Editor> = if args.editor.is_empty() {
        vec![]
    } else {
        args.editor
            .iter()
            .filter_map(|s| workspace::mcp_config::Editor::from_str(s))
            .collect()
    };

    let written = workspace::mcp_config::generate_mcp_configs(cwd, &editors)
        .context("failed to generate MCP configs")?;
    for path in &written {
        println!("created: {path}");
    }

    println!("\ninstallation complete. next steps:");
    println!("  1. edit .graphtor/config/sources.yaml to add documentation sources");
    println!("  2. run `graphtor-docs sync` to ingest");
    println!("  3. run `graphtor-docs serve` or configure your MCP client");

    Ok(0)
}

// ── doctor ────────────────────────────────────────────────────────────────────

fn cmd_doctor(cwd: &std::path::Path) -> i32 {
    let workspace_dir = cwd.join(".graphtor");
    let checks = workspace::doctor::run_doctor(&workspace_dir);

    let mut has_fail = false;
    for check in &checks {
        let icon = match check.severity {
            workspace::doctor::Severity::Pass => "✓",
            workspace::doctor::Severity::Warn => "!",
            workspace::doctor::Severity::Fail => "✗",
        };
        println!("[{icon}] {}: {}", check.severity, check.message);
        if check.severity == workspace::doctor::Severity::Fail {
            has_fail = true;
        }
    }

    if has_fail {
        2
    } else {
        0
    }
}

// ── upgrade ───────────────────────────────────────────────────────────────────

fn cmd_upgrade(cwd: &std::path::Path, args: &cli::UpgradeArgs) -> anyhow::Result<i32> {
    let workspace_dir = match workspace::paths::find_workspace_dir(cwd) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(2);
        }
    };

    let _lock = workspace::lock::WorkspaceLock::acquire(&workspace_dir, args.force_unlock)
        .context("workspace is locked by another process")?;

    let result =
        workspace::upgrade::upgrade(&workspace_dir, args.force).context("upgrade failed")?;
    if result.upgraded {
        println!("{}", result.message);
    } else {
        println!("info: {}", result.message);
    }
    Ok(0)
}

// ── uninstall ─────────────────────────────────────────────────────────────────

fn cmd_uninstall(cwd: &std::path::Path, args: &cli::UninstallArgs) -> anyhow::Result<i32> {
    if !args.confirm {
        eprintln!("error: --confirm flag is required to prevent accidental uninstall");
        eprintln!("       run: graphtor-docs uninstall --confirm");
        return Ok(2);
    }

    let ws_dir = cwd.join(".graphtor");
    let _lock = if ws_dir.exists() {
        Some(
            workspace::lock::WorkspaceLock::acquire(&ws_dir, args.force_unlock)
                .context("workspace is locked by another process")?,
        )
    } else {
        None
    };

    let result =
        workspace::uninstall::uninstall(cwd, args.keep_config).context("uninstall failed")?;

    for item in &result.removed {
        println!("removed: {item}");
    }
    println!("uninstall complete");
    Ok(0)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_workspace_source_config_produces_local_source() {
        let cwd = std::path::Path::new("/tmp/test-workspace");
        let cfg = build_workspace_source_config(cwd);
        assert_eq!(cfg.sources.len(), 1, "should produce exactly one source");
        let source = &cfg.sources[0];
        assert!(
            matches!(source, Source::Local(_)),
            "source should be a local source"
        );
        if let Source::Local(local) = source {
            assert_eq!(local.id, "workspace");
            assert!(
                local.formats.contains(&"md".to_string()),
                "formats should include md"
            );
            assert!(
                local.formats.contains(&"markdown".to_string()),
                "formats should include markdown"
            );
            assert!(
                local.include.iter().any(|p| p.contains("*.md")),
                "include should have an md glob"
            );
            assert!(
                local.exclude.iter().any(|e| e.contains(".graphtor")),
                "should exclude .graphtor/**"
            );
            assert!(
                local.exclude.iter().any(|e| e.contains(".git")),
                "should exclude .git/**"
            );
        }
    }

    #[test]
    fn load_source_config_returns_auto_discovery_when_default_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = tmp.path();
        // No sources.yaml in tmp directory — should auto-discover.
        let result = load_source_config(cwd, None).expect("load_source_config should succeed");
        let cfg = result.expect("should return Some config for auto-discovery");
        assert_eq!(cfg.sources.len(), 1, "auto-discovery returns one source");
        assert!(matches!(cfg.sources[0], Source::Local(_)));
    }

    #[test]
    fn load_source_config_returns_none_for_explicit_missing_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("nonexistent.yaml");
        let result = load_source_config(tmp.path(), Some(&missing))
            .expect("should not error for missing override");
        assert!(
            result.is_none(),
            "should return None when explicit override is missing"
        );
    }
}
