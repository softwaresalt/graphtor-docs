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
//! - `manifest`  — print a JSON-RPC 2.0 manifest of MCP tools
//! - `prewarm`   — pre-warm all documentation sources with progress reporting
//!
//! # Output format
//!
//! Pass `--json` to wrap all command output in JSON-RPC 2.0 envelopes so
//! agents can consume the results without a running MCP server.
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Context as _;
use clap::Parser as _;
use graphtor_core::mcp::{DocServer, SyncStatus};
use graphtor_core::{
    acquire::{
        execute as acquire_execute, plan as acquire_plan, AcquisitionPlan, PlannedSource,
        SourceAction,
    },
    config::SourceConfig,
    db::{list_sources, DataStore},
    init_logging,
    pipeline::FileError,
    sync::{elapsed_millis, sync_source, SyncMetrics},
    EmbeddingModel, LocalSource, LogVerbosity, PipelineConfig, Source,
};
use tracing::{error, info, warn};

use cli::{Cli, Command, OutputFormat};

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

    // Capture --json before cli is moved into run().
    let use_json = cli.json;

    let exit_code = match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            error!(error = %e, "fatal error");
            if use_json {
                println!(
                    "{}",
                    cli::jsonrpc::wrap_error(cli::jsonrpc::SERVER_ERROR, e.to_string(), None,)
                );
            } else {
                eprintln!("error: {e}");
            }
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

    let fmt = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };

    match cli.command {
        Command::Sync(args) => cmd_sync(&cwd, &db_path, sources_path.as_deref(), &args, fmt),
        Command::Serve(_) => cmd_serve(&db_path, &cwd, sources_path.as_deref()).await,
        Command::Status(args) => cmd_status(&db_path, &cwd, sources_path.as_deref(), &args, fmt),
        Command::Init(args) => cmd_init(&cwd, &args, fmt),
        Command::Install(args) => cmd_install(&cwd, &args, fmt),
        Command::Doctor => Ok(cmd_doctor(&cwd, fmt)),
        Command::Upgrade(args) => cmd_upgrade(&cwd, &args, fmt),
        Command::Uninstall(args) => cmd_uninstall(&cwd, &args, fmt),
        Command::Manifest => Ok(cmd_manifest(fmt)),
        Command::Prewarm(args) => cmd_prewarm(&cwd, &db_path, sources_path.as_deref(), &args),
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
            database: None,
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

fn source_db_path(base_db_path: &std::path::Path, source: &Source) -> PathBuf {
    source.database().map_or_else(
        || base_db_path.to_path_buf(),
        |database| {
            base_db_path
                .parent()
                .map_or_else(|| PathBuf::from(database), |parent| parent.join(database))
        },
    )
}

fn discover_db_files(base_db_path: &std::path::Path, source_config: &SourceConfig) -> Vec<PathBuf> {
    let mut db_paths = BTreeSet::new();

    for source in &source_config.sources {
        db_paths.insert(source_db_path(base_db_path, source));
    }

    if db_paths.is_empty() {
        db_paths.insert(base_db_path.to_path_buf());
    }

    db_paths.into_iter().collect()
}

fn new_grouped_plan(base_plan: &AcquisitionPlan) -> AcquisitionPlan {
    AcquisitionPlan {
        data_root: base_plan.data_root.clone(),
        allowed_root: base_plan.allowed_root.clone(),
        sources: Vec::new(),
        total_clone: 0,
        total_skip: 0,
        total_scan: 0,
        total_crawl: 0,
    }
}

fn push_grouped_source(plan: &mut AcquisitionPlan, planned: PlannedSource) {
    match &planned.action {
        SourceAction::CloneGit => plan.total_clone += 1,
        SourceAction::SkipGit => plan.total_skip += 1,
        SourceAction::ScanLocal => plan.total_scan += 1,
        SourceAction::CrawlUrl => plan.total_crawl += 1,
    }

    plan.sources.push(planned);
}

fn split_plan_by_database(
    base_db_path: &std::path::Path,
    source_config: &SourceConfig,
    plan: &AcquisitionPlan,
) -> BTreeMap<PathBuf, AcquisitionPlan> {
    let mut plans_by_db: BTreeMap<PathBuf, AcquisitionPlan> =
        discover_db_files(base_db_path, source_config)
            .into_iter()
            .map(|db_path| (db_path, new_grouped_plan(plan)))
            .collect();

    for planned in &plan.sources {
        let db_path = source_db_path(base_db_path, &planned.source);
        let grouped_plan = plans_by_db
            .entry(db_path)
            .or_insert_with(|| new_grouped_plan(plan));
        push_grouped_source(grouped_plan, planned.clone());
    }

    plans_by_db.retain(|_, grouped_plan| !grouped_plan.sources.is_empty());
    plans_by_db
}

fn cmd_sync(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    config_override: Option<&std::path::Path>,
    args: &cli::SyncArgs,
    fmt: OutputFormat,
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

    let started_at = Instant::now();
    let mut total_metrics = SyncMetrics::default();
    let mut full_sync_errors = Vec::new();

    for (target_db_path, grouped_plan) in split_plan_by_database(db_path, &source_config, &plan) {
        let store = DataStore::open_sqlite(&target_db_path, cwd)
            .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
        store
            .ensure_schema()
            .context("failed to ensure database schema")?;

        if args.full {
            let full_result = cmd_sync_full(&store, &grouped_plan, model.as_ref(), args)?;
            merge_sync_metrics(&mut total_metrics, &full_result.metrics);
            full_sync_errors.extend(full_result.errors);
        } else {
            let metrics =
                cmd_sync_incremental(&target_db_path, &store, &grouped_plan, model.as_ref());
            merge_sync_metrics(&mut total_metrics, &metrics);
        }
    }

    total_metrics.duration_ms = elapsed_millis(started_at);
    Ok(emit_sync_output(
        if args.full { "full" } else { "incremental" },
        &total_metrics,
        &full_sync_errors,
        args.metrics,
        fmt,
    ))
}

fn merge_sync_metrics(total: &mut SyncMetrics, next: &SyncMetrics) {
    total.files_total += next.files_total;
    total.files_synced += next.files_synced;
    total.files_deleted += next.files_deleted;
    total.chunks_created += next.chunks_created;
    total.chunks_deleted += next.chunks_deleted;
    total.errors += next.errors;
}

fn print_sync_metrics(metrics: &SyncMetrics) {
    println!(
        "{}",
        serde_json::to_string_pretty(metrics).expect("SyncMetrics should serialize")
    );
}

fn sync_state_path(db_path: &std::path::Path) -> PathBuf {
    // For backward compatibility: if a legacy sync_state.json exists next to
    // this DB file (from before multi-database support), keep using it so
    // existing incremental sync history is preserved.
    let legacy_path = db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("sync_state.json");
    if legacy_path.exists() {
        return legacy_path;
    }
    db_path.with_extension("sync_state.json")
}

fn emit_sync_output(
    mode: &str,
    metrics: &SyncMetrics,
    full_sync_errors: &[FileError],
    emit_metrics: bool,
    fmt: OutputFormat,
) -> i32 {
    if emit_metrics {
        print_sync_metrics(metrics);
        return i32::from(metrics.errors != 0);
    }

    if fmt == OutputFormat::Json {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "mode": mode,
                "files_processed": metrics.files_synced,
                "chunks_loaded": metrics.chunks_created,
                "files_deleted": metrics.files_deleted,
                "errors": metrics.errors,
                "metrics": metrics,
            }))
        );
        return i32::from(metrics.errors != 0);
    }

    if mode == "full" {
        println!(
            "sync complete (full): {} documents, {} chunks",
            metrics.files_synced, metrics.chunks_created
        );

        if !full_sync_errors.is_empty() {
            eprintln!("{} file(s) failed:", full_sync_errors.len());
            for file_error in full_sync_errors {
                eprintln!("  {}: {}", file_error.path.display(), file_error.error);
            }
        }
    } else {
        println!(
            "sync complete (incremental): {} files processed, {} chunks loaded, {} files deleted",
            metrics.files_synced, metrics.chunks_created, metrics.files_deleted
        );

        if metrics.errors > 0 {
            eprintln!("{} error(s) encountered during sync", metrics.errors);
        }
    }

    i32::from(metrics.errors != 0)
}

fn run_incremental_sync<F>(
    db_path: &std::path::Path,
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
    mut on_source_start: F,
) -> SyncMetrics
where
    F: FnMut(&str, usize, usize),
{
    let started_at = Instant::now();
    info!(sources = plan.sources.len(), "starting incremental sync");

    let acq_result = acquire_execute(plan, false);
    if acq_result.failed > 0 {
        warn!(
            failed = acq_result.failed,
            succeeded = acq_result.succeeded,
            "acquisition had failures; affected sources may be skipped"
        );
    }

    let state_path = sync_state_path(db_path);

    let mut total_metrics = SyncMetrics {
        errors: acq_result.failed,
        ..SyncMetrics::default()
    };
    let total_sources = plan.sources.len();

    for (index, planned) in plan.sources.iter().enumerate() {
        let source_dir = &planned.target_dir;
        let source_id = match &planned.source {
            graphtor_core::Source::Git(g) => g.id.as_str(),
            graphtor_core::Source::Local(l) => l.id.as_str(),
            graphtor_core::Source::Url(u) => u.id.as_str(),
        };

        on_source_start(source_id, index + 1, total_sources);

        if !source_dir.exists() {
            warn!(
                source_id,
                path = %source_dir.display(),
                "source directory does not exist; skipping"
            );
            total_metrics.errors += 1;
            continue;
        }

        match sync_source(
            store,
            &planned.source,
            source_dir,
            &state_path,
            &plan.allowed_root,
            model,
            None,
        ) {
            Ok(result) => merge_sync_metrics(&mut total_metrics, &result),
            Err(e) => {
                warn!(
                    source_id,
                    error = %e,
                    "incremental sync failed for source; continuing"
                );
                total_metrics.errors += 1;
            }
        }
    }

    total_metrics.duration_ms = elapsed_millis(started_at);
    total_metrics
}

#[derive(Debug)]
struct FullSyncResult {
    metrics: SyncMetrics,
    errors: Vec<FileError>,
}

/// Full pipeline: acquire → parse → embed → load all files unconditionally.
fn cmd_sync_full(
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
    args: &cli::SyncArgs,
) -> anyhow::Result<FullSyncResult> {
    let started_at = Instant::now();
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

    let error_count = result.errors_encountered.len();
    let metrics = SyncMetrics {
        files_total: result.documents_processed + error_count,
        files_synced: result.documents_processed,
        files_deleted: 0,
        chunks_created: result.total_chunks,
        chunks_deleted: 0,
        duration_ms: elapsed_millis(started_at),
        errors: error_count,
    };

    Ok(FullSyncResult {
        metrics,
        errors: result.errors_encountered,
    })
}

/// Incremental sync: acquire new sources, then detect and re-ingest only changes.
fn cmd_sync_incremental(
    db_path: &std::path::Path,
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
) -> SyncMetrics {
    run_incremental_sync(db_path, store, plan, model, |_source, _current, _total| {})
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
    stores_bg: Vec<(PathBuf, DataStore)>,
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

        let sync_status_progress = Arc::clone(&sync_status_bg);
        let result = tokio::task::spawn_blocking(move || {
            let started_at = Instant::now();
            let data_root = cwd_owned.join(".graphtor/data");
            let plan = acquire_plan::plan(&source_config, &data_root, &cwd_owned)
                .context("background sync: failed to build acquisition plan")?;

            let total_sources = plan.sources.len();
            let mut completed_sources = 0;
            let mut total_metrics = SyncMetrics::default();
            let stores_by_db: BTreeMap<PathBuf, DataStore> = stores_bg.into_iter().collect();

            for (target_db_path, grouped_plan) in
                split_plan_by_database(&db_path_owned, &source_config, &plan)
            {
                let group_source_count = grouped_plan.sources.len();
                let Some(target_store) = stores_by_db.get(&target_db_path) else {
                    anyhow::bail!(
                        "background sync store missing for database {}",
                        target_db_path.display()
                    );
                };

                let group_offset = completed_sources;
                let metrics = run_incremental_sync(
                    &target_db_path,
                    target_store,
                    &grouped_plan,
                    model_bg.as_ref(),
                    |source, current, _total| {
                        let mut guard = sync_status_progress
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *guard = SyncStatus::InProgress {
                            source: source.to_string(),
                            current: group_offset + current,
                            total: total_sources,
                        };
                    },
                );

                completed_sources += group_source_count;
                merge_sync_metrics(&mut total_metrics, &metrics);
            }

            total_metrics.duration_ms = elapsed_millis(started_at);
            Ok::<SyncMetrics, anyhow::Error>(total_metrics)
        })
        .await;

        let new_status = match result {
            Ok(Ok(metrics)) if metrics.errors == 0 => SyncStatus::Complete { metrics },
            Ok(Ok(metrics)) => SyncStatus::Error(format!(
                "{} file(s) failed during sync ({} files synced, {} chunks, {} ms)",
                metrics.errors, metrics.files_synced, metrics.chunks_created, metrics.duration_ms
            )),
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
    let source_config_result = load_source_config(cwd, config_override);
    let db_paths = match &source_config_result {
        Ok(Some(source_config)) => discover_db_files(db_path, source_config),
        Ok(None) => {
            let path = config_override.unwrap_or_else(|| std::path::Path::new("<unknown>"));
            eprintln!("error: config file '{}' not found", path.display());
            return Ok(2);
        }
        Err(e) => {
            warn!(error = %e, "failed to load source config; using primary database only");
            vec![db_path.to_path_buf()]
        }
    };

    let mut stores_by_db = Vec::new();
    for target_db_path in db_paths {
        info!(db_path = %target_db_path.display(), "opening database");
        let store = DataStore::open_sqlite(&target_db_path, cwd)
            .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
        store
            .ensure_schema()
            .context("failed to ensure database schema")?;
        stores_by_db.push((target_db_path, store));
    }

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
    let sync_status = match source_config_result {
        Ok(Some(source_config)) if !source_config.sources.is_empty() => {
            info!("background sync task spawned");
            spawn_background_sync(
                source_config,
                db_path.to_path_buf(),
                cwd.to_path_buf(),
                stores_by_db
                    .iter()
                    .map(|(path, store)| (path.clone(), store.clone()))
                    .collect(),
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

    let mut stores = stores_by_db.into_iter().map(|(_path, store)| store);
    let Some(primary) = stores.next() else {
        unreachable!("discover_db_files always yields at least one database path");
    };
    let additional: Vec<_> = stores.collect();
    let server = match model {
        Some(m) => DocServer::with_stores_and_model(primary, additional, m),
        None => DocServer::with_stores(primary, additional),
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

#[derive(Debug)]
struct StatusDatabaseEntry {
    path: PathBuf,
    sources: Vec<graphtor_core::db::SourceRecord>,
}

fn discover_status_db_paths(
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    config_override: Option<&std::path::Path>,
) -> Option<Vec<PathBuf>> {
    match load_source_config(cwd, config_override) {
        Ok(Some(source_config)) => Some(discover_db_files(db_path, &source_config)),
        Ok(None) => {
            let path = config_override.unwrap_or_else(|| std::path::Path::new("<unknown>"));
            eprintln!("error: config file '{}' not found", path.display());
            None
        }
        Err(e) => {
            warn!(error = %e, "failed to load source config; using primary database only");
            Some(vec![db_path.to_path_buf()])
        }
    }
}

fn load_status_databases(
    cwd: &std::path::Path,
    db_paths: Vec<PathBuf>,
) -> anyhow::Result<Vec<StatusDatabaseEntry>> {
    let mut databases = Vec::new();
    for candidate_db_path in db_paths {
        let sources = if candidate_db_path.exists() {
            let store = DataStore::open_sqlite(&candidate_db_path, cwd).with_context(|| {
                format!("failed to open database at {}", candidate_db_path.display())
            })?;
            store.ensure_schema().context("failed to ensure schema")?;
            list_sources(&store).context("failed to list sources")?
        } else {
            Vec::new()
        };

        databases.push(StatusDatabaseEntry {
            path: candidate_db_path,
            sources,
        });
    }
    Ok(databases)
}

fn is_missing_single_database(databases: &[StatusDatabaseEntry]) -> bool {
    databases.len() == 1 && databases[0].sources.is_empty() && !databases[0].path.exists()
}

fn status_source_json(source: &graphtor_core::db::SourceRecord) -> serde_json::Value {
    serde_json::json!({
        "id": source.source_id,
        "name": source.name,
        "kind": source.kind,
        "url": source.url,
        "synced_at": source.synced_at,
    })
}

fn status_database_json(database: &StatusDatabaseEntry) -> serde_json::Value {
    serde_json::json!({
        "database": database.path.display().to_string(),
        "sources": database.sources.iter().map(status_source_json).collect::<Vec<_>>(),
    })
}

fn emit_missing_database_status(database: &StatusDatabaseEntry, json_output: bool) {
    if json_output {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "databases": [{
                    "database": database.path.display().to_string(),
                    "sources": [],
                }],
            }))
        );
    } else {
        println!("database not found — run `graphtor-docs sync` to create it");
    }
}

fn emit_status_json(databases: &[StatusDatabaseEntry]) {
    let json_value = serde_json::json!({
        "databases": databases.iter().map(status_database_json).collect::<Vec<_>>(),
    });
    println!("{}", cli::jsonrpc::wrap_success(json_value));
}

fn print_status_database(database: &StatusDatabaseEntry) {
    println!("database: {}", database.path.display());
    println!("sources:  {}", database.sources.len());
    for source in &database.sources {
        println!(
            "  [{kind}] {id} — {url} (last sync: {synced})",
            kind = source.kind,
            id = source.source_id,
            url = source.url,
            synced = source.synced_at.as_deref().unwrap_or("never"),
        );
    }
}

fn emit_status_text(databases: &[StatusDatabaseEntry]) {
    if databases.len() == 1 {
        print_status_database(&databases[0]);
        return;
    }

    println!("databases: {}", databases.len());
    println!(
        "sources:   {}",
        databases
            .iter()
            .map(|database| database.sources.len())
            .sum::<usize>()
    );
    for database in databases {
        print_status_database(database);
    }
}

fn cmd_status(
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    config_override: Option<&std::path::Path>,
    args: &cli::StatusArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    let Some(db_paths) = discover_status_db_paths(db_path, cwd, config_override) else {
        return Ok(2);
    };
    let databases = load_status_databases(cwd, db_paths)?;
    let json_output = args.json || fmt == OutputFormat::Json;

    if is_missing_single_database(&databases) {
        emit_missing_database_status(&databases[0], json_output);
        return Ok(0);
    }

    if json_output {
        emit_status_json(&databases);
    } else {
        emit_status_text(&databases);
    }

    Ok(0)
}

// ── init ──────────────────────────────────────────────────────────────────────

fn cmd_init(cwd: &std::path::Path, args: &cli::InitArgs, fmt: OutputFormat) -> anyhow::Result<i32> {
    // Locate or create workspace dir.
    let workspace_dir = cwd.join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).context("failed to create .graphtor/")?;

    let result = workspace::init::init_sources_yaml(&workspace_dir, args.force)
        .context("failed to initialise sources.yaml")?;

    if fmt == OutputFormat::Json {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "created": result.created,
                "path": result.path.display().to_string(),
            }))
        );
        return Ok(0);
    }

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

fn cmd_install(
    cwd: &std::path::Path,
    args: &cli::InstallArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    let editors = workspace::mcp_config::parse_editors(&args.editor)?;

    // Always create the workspace directory scaffold first so the lock path exists.
    let ws_dir = cwd.join(".graphtor");
    std::fs::create_dir_all(&ws_dir).context("failed to create .graphtor directory")?;

    // Always acquire a lock to prevent concurrent installs.
    let _lock = workspace::lock::WorkspaceLock::acquire(&ws_dir, args.force_unlock)
        .context("workspace is locked by another process")?;

    let result = workspace::install::install(cwd).context("install failed")?;

    // Initialise sources.yaml (non-destructive).
    let init_result = workspace::init::init_sources_yaml(&result.workspace_dir, false)
        .context("failed to initialise sources.yaml")?;

    // Manage .gitignore (side effect only; print deferred below).
    if !args.no_gitignore {
        workspace::gitignore::add_gitignore_entry(cwd).context("failed to update .gitignore")?;
    }

    // Generate MCP client configs.
    let written = workspace::mcp_config::generate_mcp_configs(cwd, &editors)
        .context("failed to generate MCP configs")?;

    if fmt == OutputFormat::Json {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "created": result.created,
                "workspace_dir": result.workspace_dir.display().to_string(),
                "binary_path": result.binary_path.display().to_string(),
            }))
        );
        return Ok(0);
    }

    if result.created {
        println!("created: {}", result.workspace_dir.display());
    } else {
        println!(
            "workspace already exists: {}",
            result.workspace_dir.display()
        );
    }
    println!("binary:  {}", result.binary_path.display());

    if init_result.created {
        println!("created: {}", init_result.path.display());
    }

    if !args.no_gitignore {
        println!("updated: .gitignore (added .graphtor/)");
    }

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

fn cmd_doctor(cwd: &std::path::Path, fmt: OutputFormat) -> i32 {
    let workspace_dir = cwd.join(".graphtor");
    let checks = workspace::doctor::run_doctor(&workspace_dir);

    let has_fail = checks
        .iter()
        .any(|c| c.severity == workspace::doctor::Severity::Fail);
    let has_warn = checks
        .iter()
        .any(|c| c.severity == workspace::doctor::Severity::Warn);

    if fmt == OutputFormat::Json {
        let overall = if has_fail {
            "unhealthy"
        } else if has_warn {
            "degraded"
        } else {
            "healthy"
        };
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "checks": checks.iter().map(|c| {
                    let status = match c.severity {
                        workspace::doctor::Severity::Pass => "pass",
                        workspace::doctor::Severity::Warn => "warn",
                        workspace::doctor::Severity::Fail => "fail",
                    };
                    serde_json::json!({
                        "name": c.name,
                        "status": status,
                        "detail": c.message,
                    })
                }).collect::<Vec<_>>(),
                "overall": overall,
            }))
        );
        return if has_fail { 2 } else { 0 };
    }

    for check in &checks {
        let icon = match check.severity {
            workspace::doctor::Severity::Pass => "✓",
            workspace::doctor::Severity::Warn => "!",
            workspace::doctor::Severity::Fail => "✗",
        };
        println!("[{icon}] {}: {}", check.severity, check.message);
    }

    if has_fail {
        2
    } else {
        0
    }
}

// ── upgrade ───────────────────────────────────────────────────────────────────

fn cmd_upgrade(
    cwd: &std::path::Path,
    args: &cli::UpgradeArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    let workspace_dir = match workspace::paths::find_workspace_dir(cwd) {
        Ok(d) => d,
        Err(e) => {
            if fmt == OutputFormat::Json {
                println!(
                    "{}",
                    cli::jsonrpc::wrap_error(cli::jsonrpc::SERVER_ERROR, e.to_string(), None)
                );
            } else {
                eprintln!("error: {e}");
            }
            return Ok(2);
        }
    };

    let _lock = workspace::lock::WorkspaceLock::acquire(&workspace_dir, args.force_unlock)
        .context("workspace is locked by another process")?;

    let result =
        workspace::upgrade::upgrade(&workspace_dir, args.force).context("upgrade failed")?;

    if fmt == OutputFormat::Json {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "upgraded": result.upgraded,
                "message": result.message,
            }))
        );
        return Ok(0);
    }

    if result.upgraded {
        println!("{}", result.message);
    } else {
        println!("info: {}", result.message);
    }
    Ok(0)
}

// ── uninstall ─────────────────────────────────────────────────────────────────

fn cmd_uninstall(
    cwd: &std::path::Path,
    args: &cli::UninstallArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    if !args.confirm {
        if fmt == OutputFormat::Json {
            println!(
                "{}",
                cli::jsonrpc::wrap_error(
                    cli::jsonrpc::SERVER_ERROR,
                    "--confirm flag is required to prevent accidental uninstall",
                    None,
                )
            );
        } else {
            eprintln!("error: --confirm flag is required to prevent accidental uninstall");
            eprintln!("       run: graphtor-docs uninstall --confirm");
        }
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

    if fmt == OutputFormat::Json {
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({
                "removed": result.removed,
            }))
        );
        return Ok(0);
    }

    for item in &result.removed {
        println!("removed: {item}");
    }
    println!("uninstall complete");
    Ok(0)
}

// ── manifest ──────────────────────────────────────────────────────────────────

/// Print all MCP tool definitions — human-readable table or JSON-RPC 2.0 envelope.
///
/// The tool list is identical to what the `serve` subcommand advertises on the
/// MCP `tools/list` request, ensuring parity between the CLI and the STDIO
/// server interface.
fn cmd_manifest(fmt: OutputFormat) -> i32 {
    let tools = graphtor_core::mcp::list_mcp_tools();

    if fmt == OutputFormat::Json {
        let mut tool_values: Vec<serde_json::Value> = Vec::with_capacity(tools.len());
        for t in &tools {
            match serde_json::to_value(t) {
                Ok(v) => tool_values.push(v),
                Err(e) => {
                    println!(
                        "{}",
                        cli::jsonrpc::wrap_error(
                            cli::jsonrpc::SERVER_ERROR,
                            format!("failed to serialize tool '{}': {e}", t.name),
                            None,
                        )
                    );
                    return 1;
                }
            }
        }
        println!(
            "{}",
            cli::jsonrpc::wrap_success(serde_json::json!({ "tools": tool_values }))
        );
        return 0;
    }

    // Human-readable two-column table.
    println!("{:<35} Description", "Tool");
    println!("{}", "─".repeat(80));
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        println!("{:<35} {desc}", tool.name);
    }
    0
}

// ── prewarm ───────────────────────────────────────────────────────────────────

/// Pre-warm all configured documentation sources with file-level progress
/// output and JSONL telemetry.
///
/// Syncs every source in sequence, emitting `[syncing]` progress lines to
/// stderr (suppressed by `--quiet`) and a single JSONL telemetry record to
/// stdout on completion.
fn cmd_prewarm(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    config_override: Option<&std::path::Path>,
    args: &cli::prewarm::PrewarmArgs,
) -> anyhow::Result<i32> {
    let source_config: SourceConfig = if let Some(cfg) = load_source_config(cwd, config_override)? {
        cfg
    } else {
        let path = config_override.unwrap_or_else(|| std::path::Path::new("(unknown)"));
        eprintln!("error: sources.yaml not found at {}", path.display());
        return Ok(2);
    };

    if source_config.sources.is_empty() {
        warn!("sources.yaml contains no sources; nothing to prewarm");
        println!("{}", prewarm_telemetry(0, 0, 0, 0, 0));
        return Ok(0);
    }

    let data_root: PathBuf = args
        .data_root
        .clone()
        .unwrap_or_else(|| cwd.join(".graphtor/data"));
    let plan = acquire_plan::plan(&source_config, &data_root, cwd)
        .context("failed to build acquisition plan")?;

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

    let started_at = Instant::now();
    let acq_result = acquire_execute(&plan, false);
    if acq_result.failed > 0 {
        warn!(
            failed = acq_result.failed,
            succeeded = acq_result.succeeded,
            "acquisition had failures; affected sources may be skipped"
        );
    }

    let mut total_metrics = SyncMetrics {
        errors: acq_result.failed,
        ..SyncMetrics::default()
    };
    let sources_count = plan.sources.len();

    for (target_db_path, grouped_plan) in split_plan_by_database(db_path, &source_config, &plan) {
        let store = DataStore::open_sqlite(&target_db_path, cwd)
            .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
        store
            .ensure_schema()
            .context("failed to ensure database schema")?;

        let state_path = sync_state_path(&target_db_path);
        for planned in &grouped_plan.sources {
            match prewarm_sync_source(
                &store,
                planned,
                &state_path,
                &grouped_plan.allowed_root,
                model.as_ref(),
                args.quiet,
            ) {
                Some(metrics) => merge_sync_metrics(&mut total_metrics, &metrics),
                None => total_metrics.errors += 1,
            }
        }
    }

    total_metrics.duration_ms = elapsed_millis(started_at);
    println!(
        "{}",
        prewarm_telemetry(
            total_metrics.files_total,
            total_metrics.files_synced,
            total_metrics.chunks_created,
            total_metrics.duration_ms,
            sources_count,
        )
    );
    Ok(i32::from(total_metrics.errors != 0))
}

/// Sync a single planned source and return its metrics, or `None` on failure.
///
/// Emits `[syncing]` progress to stderr unless `quiet` is set.
fn prewarm_sync_source(
    store: &DataStore,
    planned: &PlannedSource,
    state_path: &std::path::Path,
    allowed_root: &std::path::Path,
    model: Option<&EmbeddingModel>,
    quiet: bool,
) -> Option<SyncMetrics> {
    let source_dir = &planned.target_dir;
    let source_id = match &planned.source {
        Source::Git(g) => g.id.as_str(),
        Source::Local(l) => l.id.as_str(),
        Source::Url(u) => u.id.as_str(),
    }
    .to_string();

    if !source_dir.exists() {
        warn!(
            source_id = %source_id,
            path = %source_dir.display(),
            "source directory does not exist; skipping"
        );
        return None;
    }

    let source_id_cb = source_id.clone();
    let mut file_cb = |path: &std::path::Path, idx: usize, total: usize| {
        if !quiet {
            let file_name = path.file_name().map_or_else(
                || path.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            let pct = (idx * 100).checked_div(total).unwrap_or(0);
            eprintln!("[syncing] {source_id_cb}: {file_name} ({idx}/{total}) [{pct}%]");
        }
    };

    match sync_source(
        store,
        &planned.source,
        source_dir,
        state_path,
        allowed_root,
        model,
        Some(&mut file_cb),
    ) {
        Ok(m) => Some(m),
        Err(e) => {
            warn!(
                source_id = %source_id,
                error = %e,
                "prewarm sync failed for source; continuing"
            );
            None
        }
    }
}

/// Build a `prewarm.complete` JSONL telemetry record.
fn prewarm_telemetry(
    files_total: usize,
    files_synced: usize,
    chunks_created: usize,
    duration_ms: u64,
    sources_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "event_type": "prewarm.complete",
        "timestamp": iso8601_now(),
        "payload": {
            "files_total": files_total,
            "files_synced": files_synced,
            "chunks_created": chunks_created,
            "duration_ms": duration_ms,
            "sources_count": sources_count,
        }
    })
}

/// Return the current UTC time as an ISO-8601 timestamp (e.g. `2026-05-21T15:30:00Z`).
#[must_use]
fn iso8601_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    epoch_secs_to_iso8601(secs)
}

/// Convert Unix epoch seconds to an ISO-8601 UTC timestamp string.
///
/// Uses the civil-date algorithm from
/// <https://howardhinnant.github.io/date_algorithms.html>.
#[must_use]
fn epoch_secs_to_iso8601(secs: u64) -> String {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3_600) % 24;
    let days = secs / 86_400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Convert days since the Unix epoch (1970-01-01) to `(year, month, day)`.
///
/// Uses the civil-date algorithm described in
/// <https://howardhinnant.github.io/date_algorithms.html>.
#[must_use]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let adj = days + 719_468;
    let era = adj / 146_097;
    let doe = adj - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_secs_to_iso8601_formats_known_date() {
        // 2026-05-21T00:00:00Z = 1_779_321_600 seconds since epoch.
        // 1970 to 2026: 56 years, with leap years 1972,1976,...,2024 (14 leaps).
        // days = 56*365 + 14 = 20440 + 14 = 20454
        // Jan(31)+Feb(28)+Mar(31)+Apr(30)+May1-20(20) = 31+28+31+30+20 = 140 days
        // days_total = 20454 + 140 = 20594
        // secs = 20594 * 86400 = 1_779_321_600  ← computed from actual epoch
        // Verified: date -d "2026-05-21" +%s = 1_779_321_600
        let secs: u64 = 1_779_321_600;
        assert_eq!(epoch_secs_to_iso8601(secs), "2026-05-21T00:00:00Z");
    }

    #[test]
    fn epoch_secs_to_iso8601_formats_unix_epoch() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_now_returns_non_empty_string() {
        let ts = iso8601_now();
        assert!(!ts.is_empty(), "timestamp should not be empty");
        assert!(
            ts.ends_with('Z'),
            "timestamp should end with Z for UTC: {ts}"
        );
    }

    #[test]
    fn cmd_install_rejects_unknown_editor_values() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let args = cli::InstallArgs {
            no_gitignore: true,
            editor: vec!["copilot".to_string()],
            force_unlock: false,
        };

        let error = cmd_install(tmp.path(), &args, OutputFormat::Human)
            .expect_err("unknown editor should fail");

        assert!(error.to_string().contains("unknown editor"));
        assert!(!tmp.path().join(".graphtor").exists());
        assert!(!tmp.path().join(".vscode/mcp.json").exists());
        assert!(!tmp.path().join(".cursor/mcp.json").exists());
    }

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

    #[test]
    fn sync_state_path_prefers_legacy_sync_state_file_when_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("graph.db");
        let legacy_path = tmp.path().join("sync_state.json");
        std::fs::write(&legacy_path, "{}").expect("write legacy sync state");

        assert_eq!(sync_state_path(&db_path), legacy_path);
    }

    #[test]
    fn sync_state_path_uses_per_database_name_when_legacy_file_is_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("graph.db");

        assert_eq!(
            sync_state_path(&db_path),
            tmp.path().join("graph.sync_state.json")
        );
    }
}
