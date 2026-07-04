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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::process;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use clap::Parser as _;
use graphtor_core::mcp::{DocServer, SyncStatus};
use graphtor_core::{
    acquire::{
        execute as acquire_execute, plan as acquire_plan, AcquisitionPlan, PlannedSource,
        SourceAction,
    },
    config::{
        discover_source_files, load_multi_file_config, load_single_file_config,
        DuplicateIntakeReport, SourceConfig,
    },
    db::{list_sources, DataStore},
    embed::{resolve_embedding_model, ResolverCaller},
    init_logging,
    pipeline::FileError,
    resolve_source_db_path,
    sync::{
        capture_pre_sync_snapshot, elapsed_millis, seed_sync_state_from_frozen_snapshot,
        seed_sync_state_from_pre_sync_snapshot, sync_source_with_frozen_mtimes_and_ignored_root,
        validate_and_begin_v4_migration_for_sources, MigrationPreflightCandidate, SyncMetrics,
        SyncProgressEvent, SyncProgressStatus, SyncState,
    },
    EmbeddingModel, LogVerbosity, PipelineConfig, Source,
};
use sha2::{Digest as _, Sha256};
use std::any::Any;
use tracing::{debug, error, info, warn};

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
    let has_explicit_db_target = cli.db_path.is_some();

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
        Command::Serve(_) => {
            cmd_serve(
                &db_path,
                &cwd,
                sources_path.as_deref(),
                has_explicit_db_target,
            )
            .await
        }
        Command::Status(args) => cmd_status(
            &db_path,
            &cwd,
            sources_path.as_deref(),
            has_explicit_db_target,
            &args,
            fmt,
        ),
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

/// Resolve a [`SourceConfig`] from an optional config-file override or auto-discovery.
///
/// Resolution order:
/// 1. `config_override` is `Some` and the file exists → load and parse it.
/// 2. `config_override` is `Some` and the file **does not** exist → return `Ok(None)`.
///    The caller is responsible for distinguishing this from case 4.  For
///    commands that accept an explicit `--config` flag, returning `Ok(None)`
///    here means the operator supplied a path that does not exist; such callers
///    SHOULD treat this as an error (see [`discover_status_db_paths`]).
/// 3. `config_override` is `None` → discover source files in `.graphtor/config/`.
/// 4. `config_override` is `None` and no config is found → return `Ok(None)`.
///    This represents the valid "workspace not yet configured" state; callers
///    MAY treat it as success with an empty result set.
///
/// Returns `Err` only when a file exists but cannot be read or parsed (always fatal).
fn load_source_config(
    cwd: &std::path::Path,
    _db_path: &std::path::Path,
    config_override: Option<&std::path::Path>,
) -> anyhow::Result<Option<SourceConfig>> {
    if let Some(path) = config_override {
        if path.exists() {
            // Use the validated single-file loader (same validation pipeline
            // as auto-discovery) rather than a bare serde_yaml::from_str, so
            // schema constraints and duplicate-source checks are enforced for
            // explicit --config overrides.  Single-file mode is correct here:
            // the `database` field is optional when only one file is supplied.
            let cfg = load_single_file_config(path)
                .with_context(|| format!("failed to load source config from {}", path.display()))?;
            return Ok(Some(cfg));
        }
        // Explicit override provided but missing — caller decides how to handle.
        return Ok(None);
    }

    let config_dir = cwd.join(".graphtor/config");

    let files = discover_source_files(&config_dir)
        .with_context(|| format!("failed to read config dir {}", config_dir.display()))?;

    if files.is_empty() {
        // No registry found — fail closed. Callers should surface an actionable error.
        return Ok(None);
    }

    let cfg = load_multi_file_config(&files)
        .with_context(|| format!("failed to load source config from {}", config_dir.display()))?;
    Ok(Some(cfg))
}

fn discover_db_files(base_db_path: &std::path::Path, source_config: &SourceConfig) -> Vec<PathBuf> {
    let mut db_paths = BTreeSet::new();

    for source in &source_config.sources {
        db_paths.insert(resolve_source_db_path(base_db_path, source));
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
        total_scan: 0,
    }
}

fn push_grouped_source(plan: &mut AcquisitionPlan, planned: PlannedSource) {
    let SourceAction::ScanLocal = &planned.action;
    plan.total_scan += 1;
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
        let db_path = resolve_source_db_path(base_db_path, &planned.source);
        let grouped_plan = plans_by_db
            .entry(db_path)
            .or_insert_with(|| new_grouped_plan(plan));
        push_grouped_source(grouped_plan, planned.clone());
    }

    plans_by_db.retain(|_, grouped_plan| !grouped_plan.sources.is_empty());
    plans_by_db
}

fn empty_acquisition_plan(
    data_root: &std::path::Path,
    allowed_root: &std::path::Path,
) -> AcquisitionPlan {
    AcquisitionPlan {
        data_root: data_root.to_path_buf(),
        allowed_root: allowed_root.to_path_buf(),
        sources: Vec::new(),
        total_scan: 0,
    }
}

fn complete_empty_registry_v4_migration_if_needed(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    data_root: &std::path::Path,
) -> anyhow::Result<bool> {
    if !db_path.exists() {
        return Ok(false);
    }

    with_locked_database_store(db_path, cwd, |store| {
        if !store
            .needs_v4_migration()
            .context("failed to determine whether database needs v4 migration")?
        {
            return Ok(false);
        }

        let pre_migration_source_ids: BTreeSet<String> = list_sources(store)
            .context("failed to capture source ids before empty-candidate v4 migration")?
            .into_iter()
            .map(|source| source.source_id)
            .collect();

        info!(
            db_path = %db_path.display(),
            "source registry is empty; running staged empty-candidate v4 migration"
        );
        let prepared =
            prepare_v4_migration_if_needed(store, empty_acquisition_plan(data_root, cwd)).map_err(
                |error| {
                    if is_blocked_live_refreeze_error(&error) {
                        error
                    } else {
                        error.context("failed to prepare staged empty-candidate v4 migration")
                    }
                },
            )?;
        if prepared.migration_started {
            clear_sync_state_after_empty_registry_v4_migration(
                cwd,
                db_path,
                &pre_migration_source_ids,
            )
            .context(
                "failed to rewrite sync state after successful empty-candidate v4 migration",
            )?;
        }
        finalize_v4_migration_if_clean(store, prepared.migration_started, 0)
            .context("failed to finalize staged empty-candidate v4 migration")?;
        Ok(prepared.migration_started)
    })
}

/// Run the duplicate-intake preflight check shared by `sync`, `prewarm`, and
/// the background-sync path in `serve`.
///
/// Returns `Ok(Some(exit_code))` when the caller should abort with that exit
/// code, or `Ok(None)` when it is safe to proceed.
///
/// `force` allows the caller to demote a duplicate conflict from a hard error
/// to a warning (only `sync --force` passes `true`; `prewarm` and `serve`
/// always pass `false`).
///
/// # Errors
///
/// Propagates any I/O or glob-compilation errors from
/// [`DuplicateIntakeReport::detect_with_context`].
fn run_duplicate_intake_preflight(
    source_config: &SourceConfig,
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    force: bool,
) -> anyhow::Result<Option<i32>> {
    let dup_report = DuplicateIntakeReport::detect_with_context(source_config, db_path, Some(cwd))
        .with_context(|| {
            format!(
                "failed duplicate-intake preflight for database {}",
                db_path.display()
            )
        })?;
    if !dup_report.is_empty() {
        if force {
            eprintln!(
                "warning: cross-database duplicate intakes detected \
                 (proceeding due to --force):\n{dup_report}"
            );
            return Ok(None);
        }
        eprintln!("error: cross-database duplicate intakes detected:\n{dup_report}");
        eprintln!("use --force to proceed anyway");
        return Ok(Some(2));
    }
    Ok(None)
}

fn handle_empty_sync_registry(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    data_root: &std::path::Path,
    args: &cli::SyncArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    warn!("source registry config contains no sources; nothing to sync");
    if complete_empty_registry_v4_migration_if_needed(cwd, db_path, data_root)? {
        return Ok(emit_sync_output(
            if args.full { "full" } else { "incremental" },
            &SyncMetrics::default(),
            &[],
            args.metrics,
            fmt,
        ));
    }
    println!("No sources configured. Add documentation sources and re-run `graphtor-docs sync`.");
    Ok(0)
}

#[allow(clippy::too_many_lines)]
fn cmd_sync(
    cwd: &std::path::Path,
    db_path: &std::path::Path,
    config_override: Option<&std::path::Path>,
    args: &cli::SyncArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    // Resolve source config: explicit override → default path → fail closed.
    let source_config: SourceConfig =
        if let Some(cfg) = load_source_config(cwd, db_path, config_override)? {
            cfg
        } else {
            // Only reachable when config_override is Some but the file does not exist,
            // or when no registry exists in .graphtor/config/.
            if let Some(path) = config_override {
                eprintln!(
                    "error: source registry config not found at {}",
                    path.display()
                );
            } else {
                eprintln!(
                    "error: no source registry config found in .graphtor/config/. \
                     Run `graphtor-docs init` to create a sources.yaml registry."
                );
            }
            return Ok(2);
        };

    let data_root: PathBuf = args
        .data_root
        .clone()
        .unwrap_or_else(|| cwd.join(".graphtor/data"));

    if source_config.sources.is_empty() {
        return handle_empty_sync_registry(cwd, db_path, &data_root, args, fmt);
    }

    // Duplicate-intake preflight (shared with prewarm / serve background sync).
    if let Some(exit_code) =
        run_duplicate_intake_preflight(&source_config, db_path, cwd, args.force)?
    {
        return Ok(exit_code);
    }

    let plan = acquire_plan::plan(&source_config, &data_root, cwd)
        .context("failed to build acquisition plan")?;

    // Load the embedding model via the shared resolver (sync/serve/prewarm parity).
    let model: Option<EmbeddingModel> =
        resolve_embedding_model(ResolverCaller::Sync, args.no_embed)
            .context("embedding model resolution failed")?;

    let started_at = Instant::now();
    let mut total_metrics = SyncMetrics::default();
    let mut full_sync_errors = Vec::new();

    for (target_db_path, grouped_plan) in split_plan_by_database(db_path, &source_config, &plan) {
        let (database_metrics, database_errors) =
            with_locked_database_store(&target_db_path, cwd, |store| {
                // Guard: --no-embed cannot be used when the database requires a v4
                // migration rebuild or when a stored epoch mismatch forces a full
                // re-ingest.  Reject early — before prepare_v4_migration_if_needed
                // prunes existing data — so the database remains intact.
                guard_no_embed_before_v4_rebuild(
                    args.no_embed,
                    store,
                    &grouped_plan,
                    &target_db_path,
                    cwd,
                )?;

                let prepared = prepare_v4_migration_if_needed(store, grouped_plan)?;
                let frozen_source_mtimes = (!prepared.frozen_source_mtimes.is_empty())
                    .then_some(&prepared.frozen_source_mtimes);

                if args.full {
                    // Capture live-source state BEFORE the pipeline runs so that
                    // any file mutation in the post-pipeline window cannot silently
                    // be recorded as already-synced.  The v4 migration path uses
                    // its own frozen snapshot (captured before the prune), so we
                    // only capture for regular (non-migration) full syncs.
                    let pre_sync_snapshot = (!prepared.migration_started)
                        .then(|| {
                            capture_pre_sync_snapshot(&prepared.rebuild_plan, cwd)
                                .context("failed to capture pre-sync snapshot before full sync")
                        })
                        .transpose()?;

                    let full_result =
                        cmd_sync_full(store, &prepared.rebuild_plan, model.as_ref(), args)?;

                    // After any successful full sync, seed sync state so the next
                    // incremental cycle has a correct baseline.  For v4 migration
                    // rebuilds use the frozen source mtimes captured before the
                    // prune; for regular full syncs use the pre-pipeline snapshot.
                    if full_result.metrics.errors == 0 {
                        let state_path = sync_state_path(&target_db_path);
                        if prepared.migration_started {
                            seed_sync_state_from_frozen_snapshot(
                                &prepared.rebuild_plan,
                                &prepared.frozen_source_mtimes,
                                &state_path,
                                cwd,
                            )
                            .context(
                                "failed to persist sync state after successful frozen v4 rebuild",
                            )?;
                        } else if let Some(ref snapshot) = pre_sync_snapshot {
                            seed_sync_state_from_pre_sync_snapshot(
                                &prepared.rebuild_plan,
                                snapshot,
                                &state_path,
                                cwd,
                            )
                            .context("failed to persist sync state after successful full sync")?;
                        }
                    }
                    finalize_v4_migration_if_clean(
                        store,
                        prepared.migration_started,
                        full_result.metrics.errors,
                    )?;
                    Ok((full_result.metrics, full_result.errors))
                } else {
                    let metrics = cmd_sync_incremental(
                        &target_db_path,
                        store,
                        &prepared.rebuild_plan,
                        model.as_ref(),
                        frozen_source_mtimes,
                    );
                    finalize_v4_migration_if_clean(
                        store,
                        prepared.migration_started,
                        metrics.errors,
                    )?;
                    Ok((metrics, Vec::new()))
                }
            })?;

        merge_sync_metrics(&mut total_metrics, &database_metrics);
        full_sync_errors.extend(database_errors);
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

fn with_locked_database_store<T, F>(
    target_db_path: &std::path::Path,
    cwd: &std::path::Path,
    action: F,
) -> anyhow::Result<T>
where
    F: FnOnce(&DataStore) -> anyhow::Result<T>,
{
    let _database_lock = acquire_database_lock(target_db_path, cwd)?;
    let store = DataStore::open_sqlite(target_db_path, cwd)
        .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
    store
        .ensure_schema()
        .context("failed to ensure database schema")?;
    action(&store)
}

fn merge_sync_metrics(total: &mut SyncMetrics, next: &SyncMetrics) {
    total.files_total += next.files_total;
    total.files_synced += next.files_synced;
    total.files_deleted += next.files_deleted;
    total.chunks_created += next.chunks_created;
    total.chunks_deleted += next.chunks_deleted;
    total.errors += next.errors;
}

#[derive(Debug)]
struct PreparedV4Migration {
    migration_started: bool,
    rebuild_plan: AcquisitionPlan,
    frozen_source_mtimes: HashMap<String, HashMap<String, u64>>,
    _snapshot_guard: Option<MigrationSnapshotGuard>,
}

#[derive(Debug)]
struct FrozenMigrationInput {
    rebuild_plan: AcquisitionPlan,
    snapshot_candidates: Vec<MigrationPreflightCandidate>,
    frozen_source_mtimes: HashMap<String, HashMap<String, u64>>,
    snapshot_guard: MigrationSnapshotGuard,
}

#[derive(Debug)]
enum SnapshotCleanupPolicy {
    DeleteAlways,
    DeleteWhenMigrationComplete(DataStore),
}

#[derive(Debug)]
struct MigrationSnapshotGuard {
    root: PathBuf,
    cleanup_policy: SnapshotCleanupPolicy,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct PersistedV4MigrationSource {
    source: Source,
    snapshot_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct PersistedV4MigrationSnapshot {
    sources: Vec<PersistedV4MigrationSource>,
    frozen_source_mtimes: HashMap<String, HashMap<String, u64>>,
}

#[must_use]
fn source_id(source: &Source) -> &str {
    let Source::Local(local) = source;
    &local.id
}

#[must_use]
fn changed_source_fields(current: &Source, persisted: &Source) -> Vec<&'static str> {
    let (Source::Local(current), Source::Local(persisted)) = (current, persisted);
    let mut changed_fields = Vec::new();
    if current.path != persisted.path {
        changed_fields.push("path");
    }
    if current.include != persisted.include {
        changed_fields.push("include");
    }
    if current.exclude != persisted.exclude {
        changed_fields.push("exclude");
    }
    if current.formats != persisted.formats {
        changed_fields.push("formats");
    }
    if current.database != persisted.database {
        changed_fields.push("database");
    }
    changed_fields
}

fn reconcile_persisted_v4_migration_sources(
    plan: &AcquisitionPlan,
    persisted: &PersistedV4MigrationSnapshot,
    snapshot_root: &std::path::Path,
) -> anyhow::Result<Vec<PlannedSource>> {
    let mut current_source_ids = BTreeSet::new();
    for planned in &plan.sources {
        let source_id = source_id(&planned.source).to_string();
        if !current_source_ids.insert(source_id.clone()) {
            anyhow::bail!(
                "current grouped plan contains duplicate source '{source_id}' while reconciling \
                 persisted v4 migration snapshot"
            );
        }
    }

    let mut persisted_sources_by_id = BTreeMap::new();
    for persisted_source in &persisted.sources {
        let source_id = source_id(&persisted_source.source).to_string();
        if persisted_sources_by_id
            .insert(source_id.clone(), persisted_source)
            .is_some()
        {
            anyhow::bail!(
                "persisted v4 migration snapshot metadata contains duplicate source \
                 '{source_id}'"
            );
        }
    }

    let persisted_source_ids = persisted_sources_by_id
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if current_source_ids != persisted_source_ids {
        let current_only = current_source_ids
            .difference(&persisted_source_ids)
            .cloned()
            .collect::<Vec<_>>();
        let snapshot_only = persisted_source_ids
            .difference(&current_source_ids)
            .cloned()
            .collect::<Vec<_>>();
        anyhow::bail!(
            "persisted v4 migration snapshot source set no longer matches the current grouped \
             plan (current only: {current_only:?}, snapshot only: {snapshot_only:?})"
        );
    }

    let persisted_mtime_ids = persisted
        .frozen_source_mtimes
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if current_source_ids != persisted_mtime_ids {
        let current_only = current_source_ids
            .difference(&persisted_mtime_ids)
            .cloned()
            .collect::<Vec<_>>();
        let snapshot_only = persisted_mtime_ids
            .difference(&current_source_ids)
            .cloned()
            .collect::<Vec<_>>();
        anyhow::bail!(
            "persisted v4 migration snapshot frozen mtime state no longer matches the current \
             grouped plan (current only: {current_only:?}, snapshot only: {snapshot_only:?})"
        );
    }

    let mut rebuild_sources = Vec::with_capacity(plan.sources.len());
    for planned in &plan.sources {
        let source_id = source_id(&planned.source);
        let persisted_source = persisted_sources_by_id.get(source_id).with_context(|| {
            format!(
                "persisted v4 migration snapshot is missing source '{source_id}' from its metadata"
            )
        })?;
        if planned.source != persisted_source.source {
            let changed_fields = changed_source_fields(&planned.source, &persisted_source.source);
            let drift_summary = if changed_fields.is_empty() {
                "changed fields: [\"unknown\"]".to_string()
            } else {
                format!("changed fields: {changed_fields:?}")
            };
            anyhow::bail!(
                "persisted v4 migration snapshot config for source '{source_id}' no longer \
                 matches the current grouped plan ({drift_summary})"
            );
        }
        let snapshot_source_dir = graphtor_core::path::validate_path(
            &snapshot_root.join(&persisted_source.snapshot_dir),
            snapshot_root,
        )
        .with_context(|| {
            format!(
                "persisted v4 migration snapshot source '{}' escaped snapshot root '{}'",
                persisted_source.snapshot_dir.display(),
                snapshot_root.display()
            )
        })?;
        let mut rebuild_source = planned.clone();
        rebuild_source.target_dir = snapshot_source_dir;
        rebuild_source.allow_internal_snapshot_scan = true;
        rebuild_sources.push(rebuild_source);
    }

    Ok(rebuild_sources)
}

impl MigrationSnapshotGuard {
    fn delete_always(root: PathBuf) -> Self {
        Self {
            root,
            cleanup_policy: SnapshotCleanupPolicy::DeleteAlways,
        }
    }

    fn delete_when_migration_complete(root: PathBuf, store: DataStore) -> Self {
        Self {
            root,
            cleanup_policy: SnapshotCleanupPolicy::DeleteWhenMigrationComplete(store),
        }
    }

    fn keep_until_migration_complete(&mut self, store: DataStore) {
        self.cleanup_policy = SnapshotCleanupPolicy::DeleteWhenMigrationComplete(store);
    }

    fn delete_on_drop(&mut self) {
        self.cleanup_policy = SnapshotCleanupPolicy::DeleteAlways;
    }
}

impl Drop for MigrationSnapshotGuard {
    fn drop(&mut self) {
        let should_remove = match &self.cleanup_policy {
            SnapshotCleanupPolicy::DeleteAlways => true,
            SnapshotCleanupPolicy::DeleteWhenMigrationComplete(store) => {
                match store.needs_v4_migration() {
                    Ok(needs_migration) => !needs_migration,
                    Err(error) => {
                        warn!(
                            path = %self.root.display(),
                            error = %error,
                            "failed to determine whether v4 migration snapshot can be removed"
                        );
                        false
                    }
                }
            }
        };

        if should_remove {
            if let Err(error) = std::fs::remove_dir_all(&self.root) {
                if self.root.exists() {
                    warn!(
                        path = %self.root.display(),
                        error = %error,
                        "failed to remove v4 migration snapshot"
                    );
                }
            }
        }
    }
}

#[derive(Debug)]
struct CollectedMigrationSource {
    planned: PlannedSource,
    files: Vec<CollectedMigrationFile>,
}

#[derive(Debug)]
struct CollectedMigrationFile {
    relative_path: PathBuf,
    live_path: PathBuf,
    live_mtime_secs: u64,
}

#[cfg(test)]
mod v4_prepare_test_hook {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    #[derive(Default)]
    struct PendingPathsState {
        owner: Option<std::thread::ThreadId>,
        paths: Vec<PathBuf>,
    }

    fn pending_paths() -> &'static Mutex<PendingPathsState> {
        static PENDING_PATHS: OnceLock<Mutex<PendingPathsState>> = OnceLock::new();
        PENDING_PATHS.get_or_init(|| Mutex::new(PendingPathsState::default()))
    }

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct PendingPathsGuard;

    impl Drop for PendingPathsGuard {
        fn drop(&mut self) {
            let mut pending = pending_paths()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.owner = None;
            pending.paths.clear();
        }
    }

    pub fn with_paths<T>(paths: Vec<PathBuf>, operation: impl FnOnce() -> T) -> T {
        let _lock_guard = test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        {
            let mut pending = pending_paths()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.owner = Some(std::thread::current().id());
            pending.paths.clear();
            pending.paths.extend(paths);
        }
        let _pending_guard = PendingPathsGuard;
        operation()
    }

    pub fn run() {
        let current_thread = std::thread::current().id();
        let Some(paths) = ({
            let mut pending = pending_paths()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if pending.owner == Some(current_thread) {
                pending.owner = None;
                Some(std::mem::take(&mut pending.paths))
            } else {
                None
            }
        }) else {
            return;
        };

        for path in paths {
            if path.exists() {
                std::fs::remove_file(&path).expect("remove live source after v4 prepare");
            }
        }
    }
}

const V4_MIGRATION_SNAPSHOT_METADATA_FILE: &str = "snapshot.json";

fn v4_migration_snapshot_root(
    data_root: &std::path::Path,
    store: &DataStore,
) -> anyhow::Result<PathBuf> {
    let db_path = store
        .database_path()
        .context("v4 migration snapshot persistence requires a file-backed database")?;
    let snapshot_id = format!("{:x}", Sha256::digest(db_path.to_string_lossy().as_bytes()));
    Ok(graphtor_core::path::v4_migration_snapshot_dir(data_root).join(format!("db-{snapshot_id}")))
}

#[must_use]
fn v4_migration_snapshot_metadata_path(snapshot_root: &std::path::Path) -> PathBuf {
    snapshot_root.join(V4_MIGRATION_SNAPSHOT_METADATA_FILE)
}

fn persist_v4_migration_snapshot(
    snapshot_root: &std::path::Path,
    snapshot: &PersistedV4MigrationSnapshot,
) -> anyhow::Result<()> {
    let metadata_path = v4_migration_snapshot_metadata_path(snapshot_root);
    let metadata = serde_json::to_string_pretty(snapshot).with_context(|| {
        format!(
            "failed to serialize v4 migration snapshot metadata '{}'",
            metadata_path.display()
        )
    })?;
    let temp_path = metadata_path.with_extension("json.tmp");
    std::fs::write(&temp_path, metadata).with_context(|| {
        format!(
            "failed to write v4 migration snapshot metadata temp file '{}'",
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, &metadata_path).with_context(|| {
        format!(
            "failed to rename v4 migration snapshot metadata temp file '{}' to '{}'",
            temp_path.display(),
            metadata_path.display()
        )
    })?;
    Ok(())
}

fn clear_persisted_v4_migration_snapshot_root(
    snapshot_root: &std::path::Path,
) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(snapshot_root) {
        Ok(metadata) => {
            let remove_result = if metadata.file_type().is_dir() {
                std::fs::remove_dir_all(snapshot_root)
            } else {
                std::fs::remove_file(snapshot_root)
            };
            remove_result.with_context(|| {
                format!(
                    "failed to remove persisted v4 migration snapshot '{}'",
                    snapshot_root.display()
                )
            })?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect persisted v4 migration snapshot '{}'",
                snapshot_root.display()
            )
        }),
    }
}

fn load_persisted_v4_migration_input(
    store: &DataStore,
    plan: &AcquisitionPlan,
) -> anyhow::Result<Option<FrozenMigrationInput>> {
    let snapshot_root = v4_migration_snapshot_root(&plan.data_root, store)
        .context("failed to determine persisted v4 migration snapshot root")?;
    if !snapshot_root.exists() {
        return Ok(None);
    }
    if !snapshot_root.is_dir() {
        anyhow::bail!(
            "persisted v4 migration snapshot root '{}' is not a directory",
            snapshot_root.display()
        );
    }

    let metadata_path = v4_migration_snapshot_metadata_path(&snapshot_root);
    if !metadata_path.exists() {
        anyhow::bail!(
            "persisted v4 migration snapshot '{}' is missing metadata '{}'; \
             reuse is required while the database remains gated",
            snapshot_root.display(),
            metadata_path.display()
        );
    }

    let metadata = std::fs::read_to_string(&metadata_path).with_context(|| {
        format!(
            "failed to read v4 migration snapshot metadata '{}'",
            metadata_path.display()
        )
    })?;
    let persisted: PersistedV4MigrationSnapshot =
        serde_json::from_str(&metadata).with_context(|| {
            format!(
                "failed to parse v4 migration snapshot metadata '{}'",
                metadata_path.display()
            )
        })?;

    let rebuild_sources =
        reconcile_persisted_v4_migration_sources(plan, &persisted, &snapshot_root).context(
            "persisted v4 migration snapshot no longer matches the current grouped plan",
        )?;

    let rebuild_plan = AcquisitionPlan {
        data_root: plan.data_root.clone(),
        allowed_root: plan.allowed_root.clone(),
        total_scan: rebuild_sources.len(),
        sources: rebuild_sources,
    };
    let snapshot_candidates =
        collect_snapshot_candidates(&rebuild_plan, Some(&persisted.frozen_source_mtimes))
            .context("failed to enumerate persisted v4 migration snapshot candidates for retry")?;

    Ok(Some(FrozenMigrationInput {
        rebuild_plan,
        snapshot_candidates,
        frozen_source_mtimes: persisted.frozen_source_mtimes,
        snapshot_guard: MigrationSnapshotGuard::delete_when_migration_complete(
            snapshot_root,
            store.clone(),
        ),
    }))
}

#[must_use]
fn blocked_live_refreeze_message(snapshot_root: &std::path::Path) -> String {
    format!(
        "persisted v4 migration snapshot '{}' cannot be reused while the database remains gated \
         after prune; refreezing from live input is blocked — restore the frozen snapshot exactly \
         or reset the staged v4 migration before retrying",
        snapshot_root.display()
    )
}

#[must_use]
fn is_blocked_live_refreeze_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .contains("refreezing from live input is blocked")
}

fn discard_unlocked_persisted_v4_migration_input(
    store: &DataStore,
    plan: &AcquisitionPlan,
) -> anyhow::Result<()> {
    let snapshot_root = v4_migration_snapshot_root(&plan.data_root, store)
        .context("failed to determine persisted v4 migration snapshot root")?;
    if !snapshot_root.exists() {
        return Ok(());
    }

    warn!(
        path = %snapshot_root.display(),
        "discarding persisted v4 migration snapshot because the database does not require snapshot reuse"
    );
    clear_persisted_v4_migration_snapshot_root(&snapshot_root).context(
        "failed to clear stale persisted v4 migration snapshot before refreezing from live input",
    )?;
    Ok(())
}

fn load_or_freeze_v4_migration_input(
    store: &DataStore,
    plan: AcquisitionPlan,
) -> anyhow::Result<FrozenMigrationInput> {
    let snapshot_reuse_required = store.v4_migration_snapshot_locked().context(
        "failed to determine whether staged v4 migration retries must reuse the persisted \
             snapshot",
    )?;
    if snapshot_reuse_required {
        return match load_persisted_v4_migration_input(store, &plan) {
            Ok(Some(existing)) => Ok(existing),
            Ok(None) => {
                let snapshot_root = v4_migration_snapshot_root(&plan.data_root, store)
                    .context("failed to determine persisted v4 migration snapshot root")?;
                anyhow::bail!("{}", blocked_live_refreeze_message(&snapshot_root));
            }
            Err(error) => {
                let snapshot_root = v4_migration_snapshot_root(&plan.data_root, store).context(
                    "failed to determine persisted v4 migration snapshot root for recovery",
                )?;
                Err(error).with_context(|| blocked_live_refreeze_message(&snapshot_root))
            }
        };
    }

    discard_unlocked_persisted_v4_migration_input(store, &plan)?;
    freeze_v4_migration_input(store, plan)
        .context("failed to freeze candidate markdown files for v4 migration rebuild")
}

fn prepare_v4_migration_if_needed(
    store: &DataStore,
    plan: AcquisitionPlan,
) -> anyhow::Result<PreparedV4Migration> {
    if !store
        .needs_v4_migration()
        .context("failed to determine whether database needs v4 migration")?
    {
        return Ok(PreparedV4Migration {
            migration_started: false,
            rebuild_plan: plan,
            frozen_source_mtimes: HashMap::new(),
            _snapshot_guard: None,
        });
    }

    let mut frozen = load_or_freeze_v4_migration_input(store, plan).map_err(|error| {
        if is_blocked_live_refreeze_error(&error) {
            error
        } else {
            error.context("failed to load or freeze v4 migration snapshot for gated retry")
        }
    })?;
    let snapshot_reuse_required = store.v4_migration_snapshot_locked().context(
        "failed to determine whether the staged v4 migration already requires persisted \
             snapshot reuse",
    )?;

    if let Err(error) =
        validate_and_begin_v4_migration_for_sources(store, &frozen.snapshot_candidates)
    {
        let is_contract_error = matches!(error, graphtor_core::GraphtorError::Contract { .. });
        if is_contract_error && !snapshot_reuse_required {
            frozen.snapshot_guard.delete_on_drop();
        } else {
            frozen
                .snapshot_guard
                .keep_until_migration_complete(store.clone());
        }
        let context = if is_contract_error {
            if snapshot_reuse_required {
                blocked_live_refreeze_message(&frozen.snapshot_guard.root)
            } else {
                "v4 migration aborted due to invalid candidate file(s); \
                 existing data preserved — fix the reported file(s) and retry"
                    .to_string()
            }
        } else {
            "failed to begin staged v4 migration rebuild from the frozen snapshot".to_string()
        };
        return Err(anyhow::Error::new(error).context(context));
    }
    frozen
        .snapshot_guard
        .keep_until_migration_complete(store.clone());
    #[cfg(test)]
    v4_prepare_test_hook::run();

    Ok(PreparedV4Migration {
        migration_started: true,
        rebuild_plan: frozen.rebuild_plan,
        frozen_source_mtimes: frozen.frozen_source_mtimes,
        _snapshot_guard: Some(frozen.snapshot_guard),
    })
}

fn finalize_v4_migration_if_clean(
    store: &DataStore,
    migration_started: bool,
    rebuild_errors: usize,
) -> anyhow::Result<()> {
    if !migration_started {
        return Ok(());
    }

    if rebuild_errors == 0 {
        store
            .mark_v4_migration_complete()
            .context("failed to mark v4 migration complete after successful rebuild")?;
    } else {
        warn!(
            rebuild_errors,
            "v4 rebuild had errors; database remains gated as pre-v4"
        );
    }

    Ok(())
}

/// Rejects `--no-embed` when the database requires a v4 migration rebuild or
/// when any planned source has a stored `contract_epoch` that differs from
/// [`graphtor_core::ingest_contract::CONTRACT_EPOCH`] (epoch-mismatch rebuild).
///
/// Both conditions force a full re-ingest that produces new chunk IDs.
/// Pre-pivot embeddings stored under old chunk IDs cannot be recovered after
/// either rebuild, so allowing `--no-embed` would permanently destroy vectors.
///
/// This guard must be called **before** `prepare_v4_migration_if_needed` so
/// the database is not pruned if the caller cannot supply embeddings for the
/// rebuilt index.
///
/// Note: the warning-only degraded-mode path for `model = None` (when the
/// flag was never explicitly set) lives in the library and is intentionally
/// kept; this guard only applies to the explicit CLI `--no-embed` flag.
fn guard_no_embed_before_v4_rebuild(
    no_embed: bool,
    store: &DataStore,
    grouped_plan: &AcquisitionPlan,
    target_db_path: &std::path::Path,
    root: &std::path::Path,
) -> anyhow::Result<()> {
    if !no_embed {
        return Ok(());
    }

    // Check 1: database requires a v4 migration rebuild.
    if store
        .needs_v4_migration()
        .context("failed to determine whether database needs v4 migration")?
    {
        anyhow::bail!(
            "--no-embed cannot be used when the database requires a v4 migration \
             rebuild; pre-pivot embeddings stored under the old chunk-ID scheme \
             cannot be recovered after the rebuild — run without --no-embed to \
             recompute embeddings under the new scheme"
        );
    }

    // Check 2: stored contract epoch mismatch forces a full re-ingest.
    // Mirror the same `epoch_changed` condition used in `sync_source` so any
    // source that would trigger a forced rebuild is caught here at the CLI layer.
    let state_path = sync_state_path(target_db_path);
    let sync_state = SyncState::load(&state_path, root).with_context(|| {
        format!(
            "failed to load sync state at {} for --no-embed epoch check",
            state_path.display()
        )
    })?;
    let has_epoch_mismatch = grouped_plan.sources.iter().any(|planned| {
        let source_id = match &planned.source {
            Source::Local(local) => local.id.as_str(),
        };
        sync_state.source(source_id).is_some_and(|s| {
            s.contract_epoch.as_deref() != Some(graphtor_core::ingest_contract::CONTRACT_EPOCH)
        })
    });
    if has_epoch_mismatch {
        anyhow::bail!(
            "--no-embed cannot be used when a stored contract epoch mismatch \
             forces a full re-ingest; pre-pivot embeddings cannot be recovered \
             after the epoch rebuild — run without --no-embed to recompute \
             embeddings under the current contract epoch"
        );
    }

    Ok(())
}

/// Collect all tracked markdown (`.md` / `.markdown`) files from the source
/// target directories in `plan`, preserving their source-relative paths and the
/// live mtimes that should be recorded after a frozen rebuild.
///
/// Uses the same fail-closed source scan as the write paths so unreadable
/// entries abort the v4 migration before any data is pruned.
fn collect_candidate_md_files(
    plan: &graphtor_core::AcquisitionPlan,
) -> anyhow::Result<Vec<CollectedMigrationSource>> {
    let mut collected_sources = Vec::with_capacity(plan.sources.len());

    for planned in &plan.sources {
        let Source::Local(local) = &planned.source;
        let scan_source = graphtor_core::LocalSource {
            id: local.id.clone(),
            path: planned.target_dir.clone(),
            include: local.include.clone(),
            exclude: local.exclude.clone(),
            formats: local.formats.clone(),
            database: local.database.clone(),
        };
        let ignored_snapshot_root = (!planned.allow_internal_snapshot_scan)
            .then(|| graphtor_core::path::v4_migration_snapshot_dir(&plan.data_root));
        let discovered_files = graphtor_core::acquire::scan_local_source_with_ignored_root(
            &scan_source,
            &plan.allowed_root,
            ignored_snapshot_root.as_deref(),
        )
        .with_context(|| {
            format!(
                "failed to scan source '{}' for v4 migration preflight",
                local.id
            )
        })?;

        let canonical_target_dir =
            graphtor_core::path::validate_path(&planned.target_dir, &plan.allowed_root)
                .with_context(|| {
                    format!(
                        "failed to validate source '{}' root for v4 migration preflight",
                        local.id
                    )
                })?;

        let mut relative_to_absolute = Vec::with_capacity(discovered_files.len());
        for absolute_path in discovered_files {
            let relative_path = absolute_path
                .strip_prefix(&canonical_target_dir)
                .map(std::path::Path::to_path_buf)
                .with_context(|| {
                    format!(
                        "failed to relativize candidate '{}' for source '{}'",
                        absolute_path.display(),
                        local.id
                    )
                })?;
            relative_to_absolute.push((relative_path, absolute_path));
        }

        let relative_paths = relative_to_absolute
            .iter()
            .map(|(relative_path, _)| relative_path.clone())
            .collect::<Vec<_>>();
        let filtered_relative_paths =
            graphtor_core::acquire::filter_files(&relative_paths, &local.include, &local.exclude)
                .with_context(|| {
                    format!(
                        "failed to filter source '{}' for v4 migration preflight",
                        local.id
                    )
                })?
                .into_iter()
                .collect::<std::collections::HashSet<_>>();

        let mut files = Vec::new();
        for (relative_path, absolute_path) in relative_to_absolute {
            if filtered_relative_paths.contains(&relative_path)
                && is_candidate_markdown_path(local, &relative_path)
            {
                files.push(CollectedMigrationFile {
                    live_mtime_secs: unix_mtime_secs(&absolute_path).with_context(|| {
                        format!(
                            "failed to read mtime for candidate '{}' in source '{}'",
                            absolute_path.display(),
                            local.id
                        )
                    })?,
                    relative_path,
                    live_path: absolute_path,
                });
            }
        }

        collected_sources.push(CollectedMigrationSource {
            planned: planned.clone(),
            files,
        });
    }

    Ok(collected_sources)
}

fn collect_snapshot_candidates(
    plan: &AcquisitionPlan,
    expected_frozen_source_mtimes: Option<&HashMap<String, HashMap<String, u64>>>,
) -> anyhow::Result<Vec<MigrationPreflightCandidate>> {
    let collected_sources = collect_candidate_md_files(plan)
        .context("failed to collect frozen snapshot candidates for v4 migration retry")?;
    let mut snapshot_candidates = Vec::new();

    for collected_source in collected_sources {
        let source_id = match &collected_source.planned.source {
            Source::Local(local) => local.id.clone(),
        };
        if let Some(expected_sources) = expected_frozen_source_mtimes {
            let expected_paths = expected_sources.get(&source_id).with_context(|| {
                format!(
                    "persisted v4 migration snapshot is missing the frozen mtime state for source '{source_id}'"
                )
            })?;
            let actual_paths = collected_source
                .files
                .iter()
                .map(|file| file.relative_path.to_string_lossy().replace('\\', "/"))
                .collect::<std::collections::HashSet<_>>();
            let expected_paths = expected_paths
                .keys()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            if actual_paths != expected_paths {
                let mut missing_paths = expected_paths
                    .difference(&actual_paths)
                    .cloned()
                    .collect::<Vec<_>>();
                missing_paths.sort();
                let mut unexpected_paths = actual_paths
                    .difference(&expected_paths)
                    .cloned()
                    .collect::<Vec<_>>();
                unexpected_paths.sort();
                anyhow::bail!(
                    "persisted v4 migration snapshot for source '{source_id}' no longer matches the \
                     original frozen file set (missing: {missing_paths:?}, unexpected: {unexpected_paths:?})"
                );
            }
        }
        snapshot_candidates.reserve(collected_source.files.len());
        for file in collected_source.files {
            snapshot_candidates.push(MigrationPreflightCandidate {
                source_id: source_id.clone(),
                path: file.live_path,
            });
        }
    }

    Ok(snapshot_candidates)
}

fn freeze_v4_migration_input(
    store: &DataStore,
    plan: AcquisitionPlan,
) -> anyhow::Result<FrozenMigrationInput> {
    let collected_sources = collect_candidate_md_files(&plan)
        .context("failed to collect candidate markdown files for v4 migration preflight")?;
    let snapshot_root = v4_migration_snapshot_root(&plan.data_root, store)
        .context("failed to determine v4 migration snapshot root")?;
    if snapshot_root.exists() {
        anyhow::bail!(
            "v4 migration snapshot root '{}' already exists before freeze; \
             retry must reuse the persisted snapshot instead of replacing it",
            snapshot_root.display()
        );
    }
    std::fs::create_dir_all(&snapshot_root).with_context(|| {
        format!(
            "failed to create v4 migration snapshot root '{}'",
            snapshot_root.display()
        )
    })?;
    let snapshot_guard = MigrationSnapshotGuard::delete_always(snapshot_root.clone());

    let mut rebuild_sources = Vec::with_capacity(collected_sources.len());
    let mut persisted_sources = Vec::with_capacity(collected_sources.len());
    let mut snapshot_candidates = Vec::new();
    let mut frozen_source_mtimes = HashMap::with_capacity(collected_sources.len());

    for (index, collected_source) in collected_sources.into_iter().enumerate() {
        let source_id = match &collected_source.planned.source {
            Source::Local(local) => local.id.clone(),
        };
        let snapshot_dir = PathBuf::from(format!("source-{index}"));
        let snapshot_source_dir = snapshot_root.join(&snapshot_dir);
        std::fs::create_dir_all(&snapshot_source_dir).with_context(|| {
            format!(
                "failed to create frozen v4 migration source dir '{}'",
                snapshot_source_dir.display()
            )
        })?;

        let mut source_mtimes = HashMap::with_capacity(collected_source.files.len());
        for file in collected_source.files {
            let snapshot_path = snapshot_source_dir.join(&file.relative_path);
            if let Some(parent) = snapshot_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create frozen v4 migration dir '{}'",
                        parent.display()
                    )
                })?;
            }
            std::fs::copy(&file.live_path, &snapshot_path).with_context(|| {
                format!(
                    "failed to freeze candidate '{}' into '{}'",
                    file.live_path.display(),
                    snapshot_path.display()
                )
            })?;

            source_mtimes.insert(
                file.relative_path.to_string_lossy().replace('\\', "/"),
                file.live_mtime_secs,
            );
            snapshot_candidates.push(MigrationPreflightCandidate {
                source_id: source_id.clone(),
                path: snapshot_path,
            });
        }

        let mut snapshot_planned = collected_source.planned;
        snapshot_planned.target_dir = snapshot_source_dir;
        snapshot_planned.allow_internal_snapshot_scan = true;
        persisted_sources.push(PersistedV4MigrationSource {
            source: snapshot_planned.source.clone(),
            snapshot_dir,
        });
        rebuild_sources.push(snapshot_planned);
        frozen_source_mtimes.insert(source_id, source_mtimes);
    }

    persist_v4_migration_snapshot(
        &snapshot_root,
        &PersistedV4MigrationSnapshot {
            sources: persisted_sources,
            frozen_source_mtimes: frozen_source_mtimes.clone(),
        },
    )
    .context("failed to persist frozen v4 migration snapshot metadata")?;

    Ok(FrozenMigrationInput {
        rebuild_plan: AcquisitionPlan {
            data_root: plan.data_root,
            allowed_root: plan.allowed_root,
            sources: rebuild_sources,
            total_scan: plan.total_scan,
        },
        snapshot_candidates,
        frozen_source_mtimes,
        snapshot_guard,
    })
}

fn unix_mtime_secs(path: &std::path::Path) -> anyhow::Result<u64> {
    let modified = path
        .metadata()
        .with_context(|| format!("failed to stat '{}'", path.display()))?
        .modified()
        .with_context(|| format!("failed to read modified time for '{}'", path.display()))?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .with_context(|| {
            format!(
                "modified time for '{}' was before the Unix epoch",
                path.display()
            )
        })?;
    Ok(secs.as_secs())
}

fn is_candidate_markdown_path(
    local: &graphtor_core::LocalSource,
    relative_path: &std::path::Path,
) -> bool {
    let Some(raw_ext) = relative_path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    let normalized_ext =
        if raw_ext.eq_ignore_ascii_case("markdown") || raw_ext.eq_ignore_ascii_case("md") {
            "md"
        } else {
            return false;
        };

    local.formats.is_empty()
        || local.formats.iter().any(|format| {
            let normalized_format = if format.eq_ignore_ascii_case("markdown") {
                "md"
            } else {
                format.as_str()
            };
            normalized_format.eq_ignore_ascii_case(normalized_ext)
        })
}

fn print_sync_metrics(metrics: &SyncMetrics) {
    println!(
        "{}",
        serde_json::to_string_pretty(metrics).expect("SyncMetrics should serialize")
    );
}

fn legacy_sync_state_path(db_path: &std::path::Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("sync_state.json")
}

fn sync_state_path(db_path: &std::path::Path) -> PathBuf {
    // For backward compatibility: if a legacy sync_state.json exists next to
    // this DB file (from before multi-database support), keep using it so
    // existing incremental sync history is preserved.
    let legacy_path = legacy_sync_state_path(db_path);
    if legacy_path.exists() {
        return legacy_path;
    }
    db_path.with_extension("sync_state.json")
}

fn clear_sync_state_after_empty_registry_v4_migration(
    root: &std::path::Path,
    db_path: &std::path::Path,
    source_ids: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let state_path = sync_state_path(db_path);
    let using_legacy_state_path = state_path == legacy_sync_state_path(db_path);
    let mut sync_state = SyncState::load(&state_path, root).with_context(|| {
        format!(
            "failed to load sync state at {} for empty-registry v4 migration",
            state_path.display()
        )
    })?;

    let removed_entries = if using_legacy_state_path && !source_ids.is_empty() {
        let mut removed = 0usize;
        for source_id in source_ids {
            removed += usize::from(sync_state.sources.remove(source_id).is_some());
        }
        removed
    } else {
        let removed = sync_state.sources.len();
        sync_state.sources.clear();
        removed
    };

    sync_state.save(&state_path, root).with_context(|| {
        format!(
            "failed to save sync state at {} after empty-registry v4 migration",
            state_path.display()
        )
    })?;

    info!(
        db_path = %db_path.display(),
        state_path = %state_path.display(),
        removed_entries,
        using_legacy_state_path,
        "cleared sync state after empty-registry v4 migration"
    );
    Ok(())
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

const SYNC_PROGRESS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

struct PeriodicHeartbeat {
    stop_tx: mpsc::Sender<()>,
    join_handle: thread::JoinHandle<()>,
}

impl PeriodicHeartbeat {
    fn spawn<F>(interval: Duration, mut on_tick: F) -> Self
    where
        F: FnMut(Duration) + Send + 'static,
    {
        let (stop_tx, stop_rx) = mpsc::channel();
        let join_handle = thread::spawn(move || {
            let started_at = Instant::now();
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => on_tick(started_at.elapsed()),
                }
            }
        });
        Self {
            stop_tx,
            join_handle,
        }
    }

    fn stop(self) {
        let _ = self.stop_tx.send(());
        if let Err(panic) = self.join_handle.join() {
            warn!(
                panic = %thread_panic_message(panic.as_ref()),
                "periodic heartbeat thread panicked"
            );
        }
    }
}

#[must_use]
fn thread_panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else {
        "unknown panic payload".to_string()
    }
}

struct SyncProgressHeartbeat {
    inner: PeriodicHeartbeat,
}

impl SyncProgressHeartbeat {
    fn start(
        tag: &'static str,
        source_id: String,
        file_path: String,
        current: usize,
        total: usize,
    ) -> Self {
        let pct = sync_progress_percent(current, total);
        let inner = PeriodicHeartbeat::spawn(SYNC_PROGRESS_HEARTBEAT_INTERVAL, move |elapsed| {
            eprintln!(
                "{tag} {source_id}: still processing {file_path} ({current}/{total}) [{pct}%] elapsed {}",
                format_sync_duration(elapsed)
            );
        });
        Self { inner }
    }

    fn stop(self) {
        self.inner.stop();
    }
}

struct CliSyncProgressReporter {
    tag: &'static str,
    source_id: String,
    enabled: bool,
    heartbeat: Option<SyncProgressHeartbeat>,
}

impl CliSyncProgressReporter {
    fn new(tag: &'static str, source_id: impl Into<String>, enabled: bool) -> Self {
        Self {
            tag,
            source_id: source_id.into(),
            enabled,
            heartbeat: None,
        }
    }

    fn handle_event(&mut self, event: SyncProgressEvent) {
        if !self.enabled {
            return;
        }

        let SyncProgressEvent {
            path,
            current,
            total,
            size_bytes,
            status,
        } = event;
        let file_path = display_sync_path(&path);
        let pct = sync_progress_percent(current, total);

        match status {
            SyncProgressStatus::Started => {
                self.finish();
                let size_suffix = size_bytes
                    .map(|size| format!(" {}", format_sync_file_size(size)))
                    .unwrap_or_default();
                eprintln!(
                    "{} {}: processing {file_path} ({current}/{total}) [{pct}%]{}",
                    self.tag, self.source_id, size_suffix
                );
                self.heartbeat = Some(SyncProgressHeartbeat::start(
                    self.tag,
                    self.source_id.clone(),
                    file_path,
                    current,
                    total,
                ));
            }
            SyncProgressStatus::Completed {
                elapsed,
                chunks_created,
            } => {
                self.finish();
                eprintln!(
                    "{} {}: completed {file_path} ({current}/{total}) [{pct}%] in {} ({} chunk(s))",
                    self.tag,
                    self.source_id,
                    format_sync_duration(elapsed),
                    chunks_created
                );
            }
            SyncProgressStatus::Failed { elapsed, error } => {
                self.finish();
                eprintln!(
                    "{} {}: failed {file_path} ({current}/{total}) [{pct}%] after {}: {error}",
                    self.tag,
                    self.source_id,
                    format_sync_duration(elapsed)
                );
            }
        }
    }

    fn finish(&mut self) {
        if let Some(active) = self.heartbeat.take() {
            active.stop();
        }
    }
}

fn sync_progress_percent(current: usize, total: usize) -> usize {
    current
        .checked_mul(100)
        .unwrap_or(0)
        .checked_div(total)
        .unwrap_or(0)
}

fn display_sync_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn format_sync_file_size(size_bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    fn format_scaled(size_bytes: u64, unit_size: u64, unit: &str) -> String {
        let whole = size_bytes / unit_size;
        let tenth = ((size_bytes % unit_size) * 10) / unit_size;
        format!("{whole}.{tenth} {unit}")
    }

    if size_bytes >= GIB {
        format_scaled(size_bytes, GIB, "GiB")
    } else if size_bytes >= MIB {
        format_scaled(size_bytes, MIB, "MiB")
    } else if size_bytes >= KIB {
        format_scaled(size_bytes, KIB, "KiB")
    } else {
        format!("{size_bytes} B")
    }
}

fn format_sync_duration(duration: Duration) -> String {
    if duration.as_secs() >= 3600 {
        let hours = duration.as_secs() / 3600;
        let minutes = (duration.as_secs() % 3600) / 60;
        let seconds = duration.as_secs() % 60;
        format!("{hours}h{minutes:02}m{seconds:02}s")
    } else if duration.as_secs() >= 60 {
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;
        format!("{minutes}m{seconds:02}s")
    } else if duration.as_secs() >= 1 {
        format!("{}.{:03}s", duration.as_secs(), duration.subsec_millis())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
mod sync_progress_tests {
    use std::io;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use super::PeriodicHeartbeat;

    struct TestLogWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl io::Write for TestLogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_warn_logs<F>(operation: F) -> String
    where
        F: FnOnce(),
    {
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(tracing::Level::WARN)
            .with_writer({
                let output = Arc::clone(&output);
                move || TestLogWriter {
                    output: Arc::clone(&output),
                }
            })
            .finish();

        tracing::subscriber::with_default(subscriber, operation);

        let bytes = output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        String::from_utf8(bytes).expect("tracing output should be valid utf-8")
    }

    #[test]
    fn periodic_heartbeat_emits_ticks_before_stop() {
        let tick_count = Arc::new(AtomicUsize::new(0));
        let on_tick = Arc::clone(&tick_count);
        let heartbeat = PeriodicHeartbeat::spawn(Duration::from_millis(1), move |_elapsed| {
            on_tick.fetch_add(1, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(25));
        heartbeat.stop();
        assert!(
            tick_count.load(Ordering::Relaxed) > 0,
            "heartbeat should emit at least one tick before stop"
        );
    }

    #[test]
    fn periodic_heartbeat_logs_warning_when_thread_panics() {
        let (tick_tx, tick_rx) = mpsc::sync_channel(1);

        let logs = capture_warn_logs(|| {
            let heartbeat = PeriodicHeartbeat::spawn(Duration::from_millis(1), move |_elapsed| {
                let _ = tick_tx.send(());
                panic!("simulated heartbeat panic");
            });

            tick_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("heartbeat should tick before timing out");
            heartbeat.stop();
        });

        assert!(
            logs.contains("periodic heartbeat thread panicked"),
            "expected panic warning in logs, got {logs:?}"
        );
        assert!(
            logs.contains("simulated heartbeat panic"),
            "expected panic payload in logs, got {logs:?}"
        );
    }
}

fn run_incremental_sync<F>(
    db_path: &std::path::Path,
    store: &DataStore,
    plan: &graphtor_core::acquire::AcquisitionPlan,
    model: Option<&EmbeddingModel>,
    frozen_source_mtimes: Option<&HashMap<String, HashMap<String, u64>>>,
    mut on_source_start: F,
    emit_file_progress: bool,
) -> SyncMetrics
where
    F: FnMut(&str, usize, usize),
{
    let started_at = Instant::now();
    info!(sources = plan.sources.len(), "starting incremental sync");
    if emit_file_progress {
        eprintln!(
            "[sync] starting incremental sync ({} source(s))",
            plan.sources.len()
        );
    }

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
            graphtor_core::Source::Local(l) => l.id.as_str(),
        };

        on_source_start(source_id, index + 1, total_sources);

        if emit_file_progress {
            eprintln!("[sync] source {}/{}: {source_id}", index + 1, total_sources);
        }

        if !source_dir.exists() {
            warn!(
                source_id,
                path = %source_dir.display(),
                "source directory does not exist; skipping"
            );
            total_metrics.errors += 1;
            continue;
        }

        let source_result = {
            let mut reporter =
                CliSyncProgressReporter::new("[sync]", source_id, emit_file_progress);
            let mut file_cb = |event: SyncProgressEvent| {
                reporter.handle_event(event);
            };
            let result = {
                let progress: graphtor_core::sync::ProgressCallback<'_> = if emit_file_progress {
                    Some(&mut file_cb)
                } else {
                    None
                };
                let ignored_snapshot_root = (!planned.allow_internal_snapshot_scan)
                    .then(|| graphtor_core::path::v4_migration_snapshot_dir(&plan.data_root));

                sync_source_with_frozen_mtimes_and_ignored_root(
                    store,
                    &planned.source,
                    source_dir,
                    &state_path,
                    &plan.allowed_root,
                    model,
                    frozen_source_mtimes.and_then(|source_mtimes| source_mtimes.get(source_id)),
                    ignored_snapshot_root.as_deref(),
                    progress,
                )
            };
            reporter.finish();
            result
        };

        match source_result {
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
    if emit_file_progress {
        eprintln!(
            "[sync] incremental sync complete: {} files processed, {} chunks loaded, {} errors",
            total_metrics.files_synced, total_metrics.chunks_created, total_metrics.errors
        );
    }
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
    eprintln!(
        "[sync-full] starting full sync: {} source(s), batch_size={}",
        plan.sources.len(),
        args.batch_size
    );

    // The pipeline executes acquire → parse → embed → load as one call; we
    // announce the combined stages around it so operators see progress
    // bracketing rather than long silent waits. Per-file granularity within
    // the pipeline is owned by the pipeline module itself.
    for stage in ["acquire", "parse", "embed", "load"] {
        eprintln!("[sync-full] stage-start: {stage}");
    }

    let result = graphtor_core::pipeline::run(plan, store, model, &pipeline_config)
        .context("pipeline execution failed")?;

    for stage in ["acquire", "parse", "embed", "load"] {
        eprintln!("[sync-full] stage-complete: {stage}");
    }

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

    eprintln!(
        "[sync-full] complete: {} documents, {} chunks, {} errors ({} ms)",
        metrics.files_synced, metrics.chunks_created, metrics.errors, metrics.duration_ms
    );

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
    frozen_source_mtimes: Option<&HashMap<String, HashMap<String, u64>>>,
) -> SyncMetrics {
    run_incremental_sync(
        db_path,
        store,
        plan,
        model,
        frozen_source_mtimes,
        |_source, _current, _total| {},
        true,
    )
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
                    None,
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
                    false,
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

/// Opened database handles for the `serve` command.
struct ServeOpenedDatabases {
    /// Advisory locks — kept alive for the duration of the server run.
    locks: Vec<workspace::lock::DatabaseLock>,
    /// Read-write stores used by the background sync task.
    rw_stores: Vec<(PathBuf, DataStore)>,
    /// Read-only stores used by the MCP query handlers.
    ro_stores: Vec<(PathBuf, DataStore)>,
}

/// Open all databases for the `serve` command.
///
/// For each path: acquires the lock, opens read-write and read-only stores,
/// ensures the schema, and gates on pre-v4 state.
///
/// Returns `Ok(None)` when all databases opened cleanly, or `Ok(Some(code))`
/// when an exit code should be returned immediately (e.g. pre-v4 gate).
fn open_serve_databases(
    db_paths: Vec<PathBuf>,
    cwd: &std::path::Path,
) -> anyhow::Result<(Option<i32>, ServeOpenedDatabases)> {
    let mut result = ServeOpenedDatabases {
        locks: Vec::new(),
        rw_stores: Vec::new(),
        ro_stores: Vec::new(),
    };
    for target_db_path in db_paths {
        info!(db_path = %target_db_path.display(), "opening database");
        let lock = acquire_database_lock(&target_db_path, cwd)?;
        let store = DataStore::open_sqlite(&target_db_path, cwd)
            .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
        store
            .ensure_schema()
            .context("failed to ensure database schema")?;
        // Gate serve on pre-v4 databases.  Serving stale pre-pivot data is
        // worse than refusing to start; the operator must run `sync` first.
        if store
            .needs_v4_migration()
            .context("failed to check database migration state")?
        {
            eprintln!(
                "error: database at '{}' has pre-v4 schema; \
                 run `graphtor-docs sync` to rebuild the index before starting serve",
                target_db_path.display()
            );
            return Ok((Some(2), result));
        }
        let readonly_store = DataStore::open_sqlite_readonly(&target_db_path, cwd)
            .with_context(|| format!("failed to open database at {}", target_db_path.display()))?;
        result.locks.push(lock);
        result.rw_stores.push((target_db_path.clone(), store));
        result.ro_stores.push((target_db_path, readonly_store));
    }
    Ok((None, result))
}

async fn cmd_serve(
    db_path: &std::path::Path,
    cwd: &std::path::Path,
    config_override: Option<&std::path::Path>,
    has_explicit_db_target: bool,
) -> anyhow::Result<i32> {
    let source_config_result = load_source_config(cwd, db_path, config_override);

    // Duplicate-intake preflight — same fail-closed check as `sync`.
    // The background-sync task spawned by `serve` is a write path: it
    // mutates databases just like an interactive sync.  Allowing it to run
    // with duplicate intakes could silently corrupt those databases.
    //
    // We check before opening any databases so that a mis-configured
    // registry is rejected immediately without creating empty DB files.
    if let Ok(Some(ref source_config)) = source_config_result {
        if !source_config.sources.is_empty() {
            if let Some(exit_code) =
                run_duplicate_intake_preflight(source_config, db_path, cwd, false)?
            {
                return Ok(exit_code);
            }
        }
    }

    let db_paths = match &source_config_result {
        Ok(Some(source_config)) => discover_db_files(db_path, source_config),
        Ok(None) => {
            if config_override.is_none() && has_explicit_db_target {
                vec![db_path.to_path_buf()]
            } else {
                let path = config_override.unwrap_or_else(|| std::path::Path::new("<unknown>"));
                eprintln!("error: config file '{}' not found", path.display());
                return Ok(2);
            }
        }
        Err(e) => {
            // A registry file exists but is malformed — fail closed.  Opening
            // databases and starting the MCP server with a broken config would
            // silently serve stale or incorrect data.  Matching the fail-closed
            // behaviour of `sync` and `status`.
            return Err(anyhow::anyhow!(
                "source registry is invalid; fix it before running serve: {e}"
            ));
        }
    };

    let (early_exit, opened) = open_serve_databases(db_paths, cwd)?;
    if let Some(code) = early_exit {
        return Ok(code);
    }
    // `_locks` keeps the advisory database locks alive for the server lifetime.
    let ServeOpenedDatabases {
        locks: _locks,
        rw_stores: stores_by_db,
        ro_stores: readonly_stores_by_db,
    } = opened;

    // Load the embedding model for semantic search via the shared resolver.
    let model: Option<EmbeddingModel> = resolve_embedding_model(ResolverCaller::Serve, false)
        .context("embedding model resolution failed")?;

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
            if config_override.is_none() && has_explicit_db_target {
                info!("no source registry found; background sync skipped");
                Arc::default()
            } else {
                // Any remaining Ok(None) case is still fail-closed: either an
                // explicit --config path is missing, or auto-discovery found no
                // registry and the operator did not explicitly target a DB.
                let path = config_override.unwrap_or_else(|| std::path::Path::new("<unknown>"));
                eprintln!("error: config file '{}' not found", path.display());
                return Ok(2);
            }
        }
        Err(e) => {
            // load_source_config Err is handled above in the db_paths match;
            // this arm is unreachable in normal operation, but kept fail-closed
            // for defence-in-depth.
            return Err(e.context("source registry is invalid; fix it before running serve"));
        }
    };

    let mut stores = readonly_stores_by_db
        .into_iter()
        .map(|(_path, store)| store);
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
    has_explicit_db_target: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    match load_source_config(cwd, db_path, config_override) {
        Ok(Some(source_config)) => Ok(discover_db_files(db_path, &source_config)),
        Ok(None) => {
            // `load_source_config` returns `Ok(None)` for two distinct reasons:
            //
            // 1. An explicit `--config` path was supplied but the file does not
            //    exist.  This is a misconfiguration — fail closed so the
            //    operator sees the bad path instead of a silent empty result.
            //
            // 2. No `--config` was supplied and auto-discovery found nothing in
            //    `.graphtor/config/`.  This is a valid "not yet configured"
            //    state — return an empty list and exit 0 unless the operator
            //    explicitly targeted a database, in which case inspect that DB.
            if let Some(path) = config_override {
                return Err(anyhow::anyhow!(
                    "source registry '{}' not found; check the --config path",
                    path.display()
                ));
            }
            if has_explicit_db_target {
                debug!(
                    db_path = %db_path.display(),
                    "no source registry found; status will inspect the explicit database target"
                );
                return Ok(vec![db_path.to_path_buf()]);
            }
            debug!("no source registry found; status will report an empty database list");
            Ok(Vec::new())
        }
        Err(e) => {
            // A registry file exists but is malformed — fail closed so the
            // operator sees the configuration error rather than stale data.
            Err(e.context("source registry is invalid; fix it before running status"))
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
            let store =
                DataStore::open_sqlite_readonly(&candidate_db_path, cwd).with_context(|| {
                    format!("failed to open database at {}", candidate_db_path.display())
                })?;
            // Gate status on pre-v4 databases.  Reporting stale pre-pivot
            // data as current is misleading; the operator must run `sync`.
            if store
                .needs_v4_migration()
                .context("failed to check database migration state")?
            {
                return Err(anyhow::anyhow!(
                    "database '{}' has pre-v4 schema; \
                     run `graphtor-docs sync` to rebuild the index",
                    candidate_db_path.display()
                ));
            }
            if store
                .relation_exists("doc_sources")
                .context("failed to inspect database schema")?
            {
                list_sources(&store).context("failed to list sources")?
            } else {
                Vec::new()
            }
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

fn acquire_database_lock(
    target_db_path: &std::path::Path,
    cwd: &std::path::Path,
) -> anyhow::Result<workspace::lock::DatabaseLock> {
    let safe_db_path =
        graphtor_core::path::validate_path(target_db_path, cwd).with_context(|| {
            format!(
                "database path '{}' must be within '{}'",
                target_db_path.display(),
                cwd.display()
            )
        })?;
    let lock_dir = safe_db_path.parent().unwrap_or(cwd);
    std::fs::create_dir_all(lock_dir).with_context(|| {
        format!(
            "failed to create database directory '{}'",
            lock_dir.display()
        )
    })?;
    workspace::lock::DatabaseLock::acquire(lock_dir, &safe_db_path, false)
        .with_context(|| format!("database '{}' is locked", target_db_path.display()))
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
    has_explicit_db_target: bool,
    args: &cli::StatusArgs,
    fmt: OutputFormat,
) -> anyhow::Result<i32> {
    let db_paths = discover_status_db_paths(db_path, cwd, config_override, has_explicit_db_target)?;
    let json_output = args.json || fmt == OutputFormat::Json;

    // No registry (empty db_paths) → emit empty databases list, exit 0.
    if db_paths.is_empty() {
        if json_output {
            println!(
                "{}",
                cli::jsonrpc::wrap_success(serde_json::json!({ "databases": [] }))
            );
        } else {
            println!("no sources configured — run `graphtor-docs init` to create sources.yaml");
        }
        return Ok(0);
    }

    let databases = load_status_databases(cwd, db_paths)?;

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

    // Generate the workspace-root .mcp.json config.
    let written =
        workspace::mcp_config::generate_mcp_config(cwd).context("failed to generate MCP config")?;

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
    let source_config: SourceConfig =
        if let Some(cfg) = load_source_config(cwd, db_path, config_override)? {
            cfg
        } else {
            let path = config_override.unwrap_or_else(|| std::path::Path::new("(unknown)"));
            eprintln!("error: sources.yaml not found at {}", path.display());
            return Ok(2);
        };

    let data_root: PathBuf = args
        .data_root
        .clone()
        .unwrap_or_else(|| cwd.join(".graphtor/data"));

    if source_config.sources.is_empty() {
        warn!("sources.yaml contains no sources; nothing to prewarm");
        let started_at = Instant::now();
        let _ = complete_empty_registry_v4_migration_if_needed(cwd, db_path, &data_root)?;
        println!(
            "{}",
            prewarm_telemetry(0, 0, 0, elapsed_millis(started_at), 0)
        );
        return Ok(0);
    }

    // Duplicate-intake preflight: same fail-closed check as `sync`.
    // `prewarm` is a write path (it mutates databases/workspaces) and must
    // never proceed when cross-database duplicate intakes are detected.
    if let Some(exit_code) = run_duplicate_intake_preflight(&source_config, db_path, cwd, false)? {
        return Ok(exit_code);
    }

    let plan = acquire_plan::plan(&source_config, &data_root, cwd)
        .context("failed to build acquisition plan")?;

    let model: Option<EmbeddingModel> =
        resolve_embedding_model(ResolverCaller::Prewarm, args.no_embed)
            .context("embedding model resolution failed")?;

    let started_at = Instant::now();
    let mut total_metrics = SyncMetrics::default();
    let sources_count = plan.sources.len();

    for (target_db_path, grouped_plan) in split_plan_by_database(db_path, &source_config, &plan) {
        let database_metrics = with_locked_database_store(&target_db_path, cwd, |store| {
            // Guard: --no-embed cannot be used when the database requires a v4
            // migration rebuild or when a stored epoch mismatch forces a full
            // re-ingest.  Reject early — before prepare_v4_migration_if_needed
            // prunes existing data — so the database remains intact.
            guard_no_embed_before_v4_rebuild(
                args.no_embed,
                store,
                &grouped_plan,
                &target_db_path,
                cwd,
            )?;
            let prepared = prepare_v4_migration_if_needed(store, grouped_plan)?;
            let acq_result = acquire_execute(&prepared.rebuild_plan, false);
            if acq_result.failed > 0 {
                warn!(
                    failed = acq_result.failed,
                    succeeded = acq_result.succeeded,
                    "acquisition had failures; affected sources may be skipped"
                );
            }

            let state_path = sync_state_path(&target_db_path);
            let mut metrics = SyncMetrics {
                errors: acq_result.failed,
                ..SyncMetrics::default()
            };
            for planned in &prepared.rebuild_plan.sources {
                let source_id = match &planned.source {
                    Source::Local(local) => local.id.as_str(),
                };
                match prewarm_sync_source(
                    store,
                    planned,
                    &state_path,
                    &prepared.rebuild_plan.allowed_root,
                    &prepared.rebuild_plan.data_root,
                    model.as_ref(),
                    prepared.frozen_source_mtimes.get(source_id),
                    args.quiet,
                ) {
                    Some(source_metrics) => merge_sync_metrics(&mut metrics, &source_metrics),
                    None => metrics.errors += 1,
                }
            }
            finalize_v4_migration_if_clean(store, prepared.migration_started, metrics.errors)?;
            Ok(metrics)
        })?;

        merge_sync_metrics(&mut total_metrics, &database_metrics);
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
#[allow(clippy::too_many_arguments)]
fn prewarm_sync_source(
    store: &DataStore,
    planned: &PlannedSource,
    state_path: &std::path::Path,
    allowed_root: &std::path::Path,
    data_root: &std::path::Path,
    model: Option<&EmbeddingModel>,
    frozen_state_mtimes: Option<&HashMap<String, u64>>,
    quiet: bool,
) -> Option<SyncMetrics> {
    let source_dir = &planned.target_dir;
    let source_id = match &planned.source {
        Source::Local(l) => l.id.as_str(),
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

    let mut reporter = CliSyncProgressReporter::new("[syncing]", source_id.clone(), !quiet);
    let mut file_cb = |event: SyncProgressEvent| {
        reporter.handle_event(event);
    };

    let result = {
        let progress: graphtor_core::sync::ProgressCallback<'_> = Some(&mut file_cb);
        let ignored_snapshot_root = (!planned.allow_internal_snapshot_scan)
            .then(|| graphtor_core::path::v4_migration_snapshot_dir(data_root));
        sync_source_with_frozen_mtimes_and_ignored_root(
            store,
            &planned.source,
            source_dir,
            state_path,
            allowed_root,
            model,
            frozen_state_mtimes,
            ignored_snapshot_root.as_deref(),
            progress,
        )
    };
    reporter.finish();

    match result {
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
    use std::fs;
    use std::path::Path;

    use super::*;
    use graphtor_core::db::{list_chunks_for_source, upsert_source, SourceRecord};
    use graphtor_core::sync::SyncState;

    fn docline_md(source_path: &str, title: &str, content: &str) -> String {
        format!(
            "---\ntitle: {title}\nsource: /test/source\ningested_at: \
             2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
        )
    }

    fn seed_store_at_v3_with_source(db_path: &Path, root: &Path, source_id: &str) {
        let store = DataStore::open_sqlite(db_path, root).expect("open sqlite store");
        store.ensure_schema().expect("ensure schema");
        store
            .set_schema_version_for_test(3)
            .expect("set version to 3");
        upsert_source(
            &store,
            &SourceRecord {
                source_id: source_id.to_string(),
                url: "file:///docs".to_string(),
                kind: "local".to_string(),
                name: source_id.to_string(),
                synced_at: None,
            },
        )
        .expect("seed legacy source");
    }

    fn seed_store_at_v3(db_path: &Path, root: &Path) {
        seed_store_at_v3_with_source(db_path, root, "legacy-source");
    }

    fn write_single_source_config(root: &Path, source_id: &str, source_dir: &Path) {
        let config_dir = root.join(".graphtor").join("config");
        fs::create_dir_all(&config_dir).expect("create config dir");
        let source_dir_yaml = source_dir.display().to_string().replace('\\', "/");
        fs::write(
            config_dir.join("sources.yaml"),
            format!(
                "sources:\n  - type: local\n    id: {source_id}\n    path: {source_dir_yaml}\n    include:\n      - \"**/*.md\"\n"
            ),
        )
        .expect("write sources.yaml");
    }

    fn write_empty_source_config(root: &Path) {
        let config_dir = root.join(".graphtor").join("config");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("sources.yaml"), "sources: []\n")
            .expect("write empty sources.yaml");
    }

    fn seed_current_sync_state(
        root: &Path,
        db_path: &Path,
        source_id: &str,
        relative_path: &str,
        mtime: u64,
    ) {
        let mut state = SyncState::default();
        let source_state = state.source_mut(source_id);
        source_state
            .file_mtimes
            .insert(relative_path.to_string(), mtime);
        source_state
            .file_contract_paths
            .insert(relative_path.to_string(), relative_path.to_string());
        source_state.last_sync = Some("2026-01-01T00:00:00Z".to_string());
        source_state.contract_epoch =
            Some(graphtor_core::ingest_contract::CONTRACT_EPOCH.to_string());
        state
            .save(&sync_state_path(db_path), root)
            .expect("seed sync state");
    }

    fn assert_source_loaded(root: &Path, db_path: &Path, source_id: &str) {
        let store = DataStore::open_sqlite(db_path, root).expect("reopen sqlite store");
        assert!(
            !store.needs_v4_migration().expect("check migration gate"),
            "migration gate should clear after successful rebuild"
        );
        let chunks = list_chunks_for_source(&store, source_id).expect("list chunks");
        assert!(
            !chunks.is_empty(),
            "frozen rebuild should load chunks for source '{source_id}'"
        );
        assert!(
            chunks.iter().any(|chunk| chunk.path == "guide.md"),
            "expected guide.md chunk(s), got: {chunks:?}"
        );
    }

    fn assert_invalid_persisted_v4_snapshot_retry_fails_closed(
        mutate_snapshot: impl FnOnce(&Path, &Path),
        assert_snapshot_after_failure: impl FnOnce(&Path, &Path),
    ) {
        super::v4_prepare_test_hook::with_paths(Vec::new(), || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = tmp.path();
            let docs_dir = root.join("docs");
            fs::create_dir_all(&docs_dir).expect("create docs dir");
            let live_file = docs_dir.join("guide.md");
            fs::write(
                &live_file,
                docline_md("guide.md", "Guide", "# Guide\n\nOriginal content.\n"),
            )
            .expect("write guide.md");

            let db_path = root.join("graph.db");
            seed_store_at_v3(&db_path, root);

            let plan = AcquisitionPlan {
                data_root: root.join(".graphtor").join("data"),
                allowed_root: root.to_path_buf(),
                sources: vec![PlannedSource {
                    source: Source::Local(graphtor_core::LocalSource {
                        id: "guide-source".to_string(),
                        path: docs_dir.clone(),
                        include: vec!["**/*.md".to_string()],
                        exclude: vec![],
                        formats: vec!["md".to_string()],
                        database: None,
                    }),
                    action: SourceAction::ScanLocal,
                    target_dir: docs_dir.clone(),
                    allow_internal_snapshot_scan: false,
                }],
                total_scan: 1,
            };

            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            let prepared = prepare_v4_migration_if_needed(&store, plan.clone())
                .expect("initial v4 migration prepare should succeed");
            finalize_v4_migration_if_clean(&store, prepared.migration_started, 1)
                .expect("errorful rebuild should leave the migration gated");

            let snapshot_dir = prepared.rebuild_plan.sources[0].target_dir.clone();
            let snapshot_root = snapshot_dir
                .parent()
                .expect("snapshot source dir should have a parent")
                .to_path_buf();
            let snapshot_file = snapshot_dir.join("guide.md");
            drop(prepared);

            fs::write(
                &live_file,
                docline_md("guide.md", "Guide", "# Guide\n\nFresh content.\n"),
            )
            .expect("rewrite live guide.md");
            mutate_snapshot(&snapshot_root, &snapshot_file);

            let retry_error = prepare_v4_migration_if_needed(&store, plan)
                .expect_err("unusable persisted snapshot must fail closed after prune");
            let retry_message = retry_error.to_string();
            assert!(
                retry_message.contains("refreezing from live input is blocked"),
                "retry error must explain that live fallback is forbidden after prune: {retry_message}"
            );
            assert!(
                store.needs_v4_migration().expect("check migration gate"),
                "failed retry must leave the database gated until the frozen snapshot is repaired"
            );
            assert!(
                snapshot_root.exists(),
                "failed retry must preserve the persisted snapshot root for operator inspection"
            );
            assert_snapshot_after_failure(&snapshot_root, &snapshot_file);
        });
    }

    fn planned_local_source(source_id: &str, source_dir: &Path) -> PlannedSource {
        PlannedSource {
            source: Source::Local(graphtor_core::LocalSource {
                id: source_id.to_string(),
                path: source_dir.to_path_buf(),
                include: vec!["**/*.md".to_string()],
                exclude: vec![],
                formats: vec!["md".to_string()],
                database: None,
            }),
            action: SourceAction::ScanLocal,
            target_dir: source_dir.to_path_buf(),
            allow_internal_snapshot_scan: false,
        }
    }

    fn local_plan(root: &Path, sources: Vec<PlannedSource>) -> AcquisitionPlan {
        let total_scan = sources.len();
        AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources,
            total_scan,
        }
    }

    fn load_persisted_v4_migration_snapshot(snapshot_root: &Path) -> PersistedV4MigrationSnapshot {
        let metadata_path = v4_migration_snapshot_metadata_path(snapshot_root);
        let metadata =
            fs::read_to_string(&metadata_path).expect("read persisted snapshot metadata");
        serde_json::from_str(&metadata).expect("parse persisted snapshot metadata")
    }

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
    fn cmd_install_writes_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let args = cli::InstallArgs {
            no_gitignore: true,
            force_unlock: false,
        };

        let code =
            cmd_install(tmp.path(), &args, OutputFormat::Human).expect("install should succeed");

        assert_eq!(code, 0);
        assert!(tmp.path().join(".mcp.json").exists());
        assert!(!tmp.path().join(".vscode/mcp.json").exists());
        assert!(!tmp.path().join(".cursor/mcp.json").exists());
        assert!(!tmp.path().join(".github/copilot/mcp.json").exists());
    }

    #[test]
    fn load_source_config_returns_none_when_no_registry_found() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cwd = tmp.path();
        // No sources.yaml in tmp directory — should fail closed with None.
        let db_path = cwd.join("graph.db");
        let result =
            load_source_config(cwd, &db_path, None).expect("load_source_config should succeed");
        assert!(
            result.is_none(),
            "should return None when no registry found"
        );
    }

    #[test]
    fn load_source_config_returns_none_for_explicit_missing_override() {
        // `load_source_config` itself returns `Ok(None)` for an explicit
        // missing path — it is the responsibility of callers that accept
        // `--config` to convert this into an error.  `discover_status_db_paths`
        // does exactly that; the integration test
        // `status_fails_closed_when_explicit_config_is_missing` covers the
        // end-to-end fail-closed guarantee for the `status` subcommand.
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("graph.db"); // does not exist
        let missing = tmp.path().join("nonexistent.yaml");
        let result = load_source_config(tmp.path(), &db_path, Some(&missing))
            .expect("should not error for missing override");
        assert!(
            result.is_none(),
            "should return None when explicit override is missing"
        );
    }

    #[test]
    fn load_status_databases_returns_empty_sources_for_uninitialized_database() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("partial.db");

        let _store = DataStore::open_sqlite(&db_path, tmp.path())
            .expect("open_sqlite should create the database file");

        let databases = load_status_databases(tmp.path(), vec![db_path.clone()])
            .expect("status should tolerate an uninitialized database");

        assert_eq!(databases.len(), 1, "expected one database entry");
        assert_eq!(databases[0].path, db_path);
        assert!(
            databases[0].sources.is_empty(),
            "uninitialized database should report no sources"
        );
    }

    #[test]
    fn with_locked_database_store_releases_lock_after_callback_returns() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("graph.db");
        let lock_path = tmp.path().join("graph.db.lock");

        with_locked_database_store(&db_path, tmp.path(), |_store| {
            assert!(lock_path.exists(), "lock file should exist during callback");
            Ok(())
        })
        .expect("locked store helper should succeed");

        assert!(
            !lock_path.exists(),
            "lock file should be removed after callback returns"
        );

        let _lock = acquire_database_lock(&db_path, tmp.path())
            .expect("lock should be acquirable after callback returns");
    }

    #[test]
    fn collect_candidate_md_files_fails_closed_when_source_is_not_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source_file = tmp.path().join("docs.md");
        std::fs::write(
            &source_file,
            b"---\ntitle: Guide\nsource: /test/source\ningested_at: \
              2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n",
        )
        .expect("write source file");

        let plan = graphtor_core::AcquisitionPlan {
            data_root: tmp.path().join(".graphtor").join("data"),
            allowed_root: tmp.path().to_path_buf(),
            sources: vec![graphtor_core::PlannedSource {
                source: Source::Local(graphtor_core::LocalSource {
                    id: "file-source".to_string(),
                    path: source_file.clone(),
                    include: vec![],
                    exclude: vec![],
                    formats: vec!["md".to_string()],
                    database: None,
                }),
                action: graphtor_core::SourceAction::ScanLocal,
                target_dir: source_file,
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        let error =
            collect_candidate_md_files(&plan).expect_err("source scan error must be surfaced");
        assert!(
            error
                .to_string()
                .contains("failed to scan source 'file-source'"),
            "expected a source scan error, got: {error}"
        );
    }

    #[test]
    fn unlocked_persisted_v4_snapshot_is_discarded_and_refrozen_from_live_input() {
        super::v4_prepare_test_hook::with_paths(Vec::new(), || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = tmp.path();
            let docs_dir = root.join("docs");
            fs::create_dir_all(&docs_dir).expect("create docs dir");
            let live_file = docs_dir.join("guide.md");
            fs::write(
                &live_file,
                docline_md("guide.md", "Guide", "# Guide\n\nOriginal content.\n"),
            )
            .expect("write original live guide");

            let db_path = root.join("graph.db");
            seed_store_at_v3(&db_path, root);

            let plan = local_plan(root, vec![planned_local_source("guide-source", &docs_dir)]);
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");

            let mut stale = freeze_v4_migration_input(&store, plan.clone())
                .expect("freeze an initial persisted snapshot");
            let snapshot_dir = stale.rebuild_plan.sources[0].target_dir.clone();
            let snapshot_root = snapshot_dir
                .parent()
                .expect("snapshot source dir should have a parent")
                .to_path_buf();
            let snapshot_file = snapshot_dir.join("guide.md");
            let stale_contents =
                fs::read_to_string(&snapshot_file).expect("read initial persisted snapshot");
            assert!(
                stale_contents.contains("Original content."),
                "pre-condition: stale snapshot should contain the original live content"
            );
            assert!(
                !store
                    .v4_migration_snapshot_locked()
                    .expect("check unlocked snapshot state before staged prune"),
                "pre-condition: snapshot reuse must not be required before prune starts"
            );

            stale
                .snapshot_guard
                .keep_until_migration_complete(store.clone());
            drop(stale);
            assert!(
                snapshot_root.exists(),
                "simulated stale persisted snapshot should remain on disk before the next retry"
            );

            fs::write(
                &live_file,
                docline_md("guide.md", "Guide", "# Guide\n\nFresh content.\n"),
            )
            .expect("rewrite live guide with fresh content");

            let prepared = prepare_v4_migration_if_needed(&store, plan)
                .expect("prepare should discard the stale snapshot and refreeze from live input");
            assert!(
                prepared.migration_started,
                "pre-v4 store should still begin staged migration"
            );

            let refrozen_contents =
                fs::read_to_string(prepared.rebuild_plan.sources[0].target_dir.join("guide.md"))
                    .expect("read refrozen snapshot file");
            assert!(
                refrozen_contents.contains("Fresh content."),
                "prepare should refreeze from live input when snapshot reuse is not required: \
                 {refrozen_contents}"
            );
            assert!(
                !refrozen_contents.contains("Original content."),
                "stale persisted snapshots must not be reused before prune starts"
            );

            finalize_v4_migration_if_clean(&store, prepared.migration_started, 0)
                .expect("complete staged migration after refreeze");
            drop(prepared);
            assert!(
                !snapshot_root.exists(),
                "successful completion should clean up the refrozen snapshot"
            );
        });
    }

    #[test]
    fn v4_migration_snapshot_survives_gated_retry_until_successful_cleanup() {
        super::v4_prepare_test_hook::with_paths(Vec::new(), || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = tmp.path();
            let docs_dir = root.join("docs");
            fs::create_dir_all(&docs_dir).expect("create docs dir");
            let live_file = docs_dir.join("guide.md");
            fs::write(
                &live_file,
                docline_md("guide.md", "Guide", "# Guide\n\nFrozen content.\n"),
            )
            .expect("write guide.md");
            let live_mtime = unix_mtime_secs(&live_file).expect("read live mtime");

            let db_path = root.join("graph.db");
            seed_store_at_v3(&db_path, root);

            let plan = AcquisitionPlan {
                data_root: root.join(".graphtor").join("data"),
                allowed_root: root.to_path_buf(),
                sources: vec![PlannedSource {
                    source: Source::Local(graphtor_core::LocalSource {
                        id: "guide-source".to_string(),
                        path: docs_dir.clone(),
                        include: vec!["**/*.md".to_string()],
                        exclude: vec![],
                        formats: vec!["md".to_string()],
                        database: None,
                    }),
                    action: SourceAction::ScanLocal,
                    target_dir: docs_dir.clone(),
                    allow_internal_snapshot_scan: false,
                }],
                total_scan: 1,
            };

            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");

            let prepared = prepare_v4_migration_if_needed(&store, plan.clone())
                .expect("initial v4 migration prepare should succeed");
            assert!(
                prepared.migration_started,
                "migration should start for pre-v4 store"
            );

            let first_snapshot_dir = prepared.rebuild_plan.sources[0].target_dir.clone();
            let first_snapshot_root = first_snapshot_dir
                .parent()
                .expect("snapshot source dir should have a parent")
                .to_path_buf();
            let first_snapshot_file = first_snapshot_dir.join("guide.md");
            assert!(
                first_snapshot_file.exists(),
                "initial prepare should freeze the live file into the snapshot"
            );
            assert_eq!(
                prepared
                    .frozen_source_mtimes
                    .get("guide-source")
                    .and_then(|source| source.get("guide.md")),
                Some(&live_mtime),
                "initial prepare must capture the live-source mtime"
            );

            finalize_v4_migration_if_clean(&store, prepared.migration_started, 1)
                .expect("errorful rebuild should leave the migration gated");
            drop(prepared);

            assert!(
                store.needs_v4_migration().expect("check migration gate"),
                "migration gate must remain active after rebuild errors"
            );
            assert!(
                first_snapshot_file.exists(),
                "failed rebuild must keep the original frozen snapshot for retry"
            );

            fs::remove_file(&live_file).expect("remove live guide after first freeze");

            let retry = prepare_v4_migration_if_needed(&store, plan)
                .expect("retry prepare should reuse the original frozen snapshot");
            assert!(
                retry.migration_started,
                "retry should still run the staged migration while the gate is active"
            );
            let canonical_first_snapshot_dir =
                graphtor_core::path::validate_path(&first_snapshot_dir, root)
                    .expect("canonicalize first snapshot dir");
            let canonical_retry_snapshot_dir =
                graphtor_core::path::validate_path(&retry.rebuild_plan.sources[0].target_dir, root)
                    .expect("canonicalize retry snapshot dir");
            assert_eq!(
                canonical_retry_snapshot_dir, canonical_first_snapshot_dir,
                "retry must point at the original frozen snapshot directory"
            );
            assert!(
                retry.rebuild_plan.sources[0]
                    .target_dir
                    .join("guide.md")
                    .exists(),
                "retry plan must still include the frozen file after it disappears from the live tree"
            );
            assert_eq!(
                retry
                    .frozen_source_mtimes
                    .get("guide-source")
                    .and_then(|source| source.get("guide.md")),
                Some(&live_mtime),
                "retry must preserve the original live-source mtime snapshot"
            );

            finalize_v4_migration_if_clean(&store, retry.migration_started, 0)
                .expect("clean rebuild should complete the staged migration");
            drop(retry);

            assert!(
                !store
                    .needs_v4_migration()
                    .expect("check cleared migration gate"),
                "migration gate should clear after a clean rebuild"
            );
            assert!(
                !first_snapshot_root.exists(),
                "successful completion should clean up the frozen snapshot root"
            );
        });
    }

    #[test]
    fn persisted_v4_snapshot_contract_corruption_fails_closed_after_prune() {
        assert_invalid_persisted_v4_snapshot_retry_fails_closed(
            |_snapshot_root, snapshot_file| {
                fs::write(snapshot_file, "# missing contract frontmatter\n")
                    .expect("corrupt persisted snapshot file");
            },
            |_snapshot_root, snapshot_file| {
                let corrupt_snapshot =
                    fs::read_to_string(snapshot_file).expect("read corrupt persisted snapshot");
                assert!(
                    corrupt_snapshot.contains("missing contract frontmatter"),
                    "failed retry must preserve the corrupt frozen file for repair: {corrupt_snapshot}"
                );
            },
        );
    }

    #[test]
    fn persisted_v4_snapshot_missing_metadata_fails_closed_after_prune() {
        assert_invalid_persisted_v4_snapshot_retry_fails_closed(
            |snapshot_root, _snapshot_file| {
                let metadata_path = v4_migration_snapshot_metadata_path(snapshot_root);
                fs::remove_file(&metadata_path).expect("remove persisted snapshot metadata");
            },
            |snapshot_root, _snapshot_file| {
                let metadata_path = v4_migration_snapshot_metadata_path(snapshot_root);
                assert!(
                    !metadata_path.exists(),
                    "failed retry must not recreate missing metadata from live input"
                );
            },
        );
    }

    #[test]
    fn persisted_v4_snapshot_missing_frozen_file_fails_closed_after_prune() {
        assert_invalid_persisted_v4_snapshot_retry_fails_closed(
            |_snapshot_root, snapshot_file| {
                fs::remove_file(snapshot_file).expect("remove frozen snapshot file");
            },
            |_snapshot_root, snapshot_file| {
                assert!(
                    !snapshot_file.exists(),
                    "failed retry must not recreate missing frozen files from live input"
                );
            },
        );
    }

    #[test]
    fn persisted_v4_snapshot_grouped_source_set_drift_fails_closed_after_prune() {
        super::v4_prepare_test_hook::with_paths(Vec::new(), || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = tmp.path();
            let docs_a = root.join("docs-a");
            let docs_b = root.join("docs-b");
            fs::create_dir_all(&docs_a).expect("create docs-a dir");
            fs::create_dir_all(&docs_b).expect("create docs-b dir");
            fs::write(
                docs_a.join("guide-a.md"),
                docline_md("guide-a.md", "Guide A", "# Guide A\n\nCurrent content.\n"),
            )
            .expect("write docs-a guide");
            fs::write(
                docs_b.join("guide-b.md"),
                docline_md("guide-b.md", "Guide B", "# Guide B\n\nCurrent content.\n"),
            )
            .expect("write docs-b guide");

            let db_path = root.join("graph.db");
            seed_store_at_v3(&db_path, root);

            let initial_plan = local_plan(
                root,
                vec![
                    planned_local_source("alpha-source", &docs_a),
                    planned_local_source("beta-source", &docs_b),
                ],
            );
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            let prepared = prepare_v4_migration_if_needed(&store, initial_plan)
                .expect("initial v4 migration prepare should succeed");
            finalize_v4_migration_if_clean(&store, prepared.migration_started, 1)
                .expect("errorful rebuild should leave the migration gated");

            let snapshot_root = prepared.rebuild_plan.sources[0]
                .target_dir
                .parent()
                .expect("snapshot source dir should have a parent")
                .to_path_buf();
            drop(prepared);

            let retry_plan = local_plan(root, vec![planned_local_source("alpha-source", &docs_a)]);
            let retry_error = prepare_v4_migration_if_needed(&store, retry_plan)
                .expect_err("grouped source drift must fail closed after prune");
            let retry_message = retry_error.to_string();
            assert!(
                retry_message.contains("refreezing from live input is blocked"),
                "retry error must explain that live fallback is forbidden after prune: {retry_message}"
            );
            assert!(
                store.needs_v4_migration().expect("check migration gate"),
                "failed retry must leave the migration gate active"
            );
            assert!(
                snapshot_root.exists(),
                "failed retry must preserve the persisted snapshot for operator repair"
            );

            let persisted = load_persisted_v4_migration_snapshot(&snapshot_root);
            assert_eq!(
                persisted.sources.len(),
                2,
                "failed retry must preserve the original frozen source set"
            );
            assert_eq!(
                persisted
                    .sources
                    .iter()
                    .map(|source| source_id(&source.source))
                    .collect::<Vec<_>>(),
                vec!["alpha-source", "beta-source"],
                "failed retry must preserve the original grouped-plan sources"
            );
            assert!(
                persisted.frozen_source_mtimes.contains_key("beta-source"),
                "failed retry must preserve frozen mtimes for the removed source"
            );
        });
    }

    #[test]
    fn persisted_v4_snapshot_grouped_source_config_drift_fails_closed_after_prune() {
        super::v4_prepare_test_hook::with_paths(Vec::new(), || {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = tmp.path();
            let docs_old = root.join("docs-old");
            let docs_new = root.join("docs-new");
            fs::create_dir_all(&docs_old).expect("create docs-old dir");
            fs::create_dir_all(&docs_new).expect("create docs-new dir");
            fs::write(
                docs_old.join("guide.md"),
                docline_md("guide.md", "Guide", "# Guide\n\nOriginal content.\n"),
            )
            .expect("write old guide");
            fs::write(
                docs_new.join("guide.md"),
                docline_md("guide.md", "Guide", "# Guide\n\nFresh content.\n"),
            )
            .expect("write new guide");

            let db_path = root.join("graph.db");
            seed_store_at_v3(&db_path, root);

            let initial_plan =
                local_plan(root, vec![planned_local_source("guide-source", &docs_old)]);
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            let prepared = prepare_v4_migration_if_needed(&store, initial_plan)
                .expect("initial v4 migration prepare should succeed");
            finalize_v4_migration_if_clean(&store, prepared.migration_started, 1)
                .expect("errorful rebuild should leave the migration gated");

            let snapshot_root = prepared.rebuild_plan.sources[0]
                .target_dir
                .parent()
                .expect("snapshot source dir should have a parent")
                .to_path_buf();
            drop(prepared);

            let retry_plan =
                local_plan(root, vec![planned_local_source("guide-source", &docs_new)]);
            let retry_error = prepare_v4_migration_if_needed(&store, retry_plan)
                .expect_err("source config drift must fail closed after prune");
            let retry_message = retry_error.to_string();
            assert!(
                retry_message.contains("refreezing from live input is blocked"),
                "retry error must explain that live fallback is forbidden after prune: {retry_message}"
            );
            assert!(
                store.needs_v4_migration().expect("check migration gate"),
                "failed retry must leave the migration gate active"
            );
            assert!(
                snapshot_root.exists(),
                "failed retry must preserve the persisted snapshot for operator repair"
            );

            let persisted = load_persisted_v4_migration_snapshot(&snapshot_root);
            let Source::Local(local) = &persisted.sources[0].source;
            assert_eq!(
                local.path, docs_old,
                "failed retry must preserve the original grouped-plan source config"
            );
        });
    }

    #[test]
    fn cmd_sync_full_v4_migration_uses_frozen_snapshot_after_live_source_removed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nFrozen content.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: false, // must not use --no-embed during v4 migration rebuild
            data_root: None,
            full: true,
            metrics: false,
            force: false,
        };

        let exit_code = super::v4_prepare_test_hook::with_paths(vec![live_file], || {
            cmd_sync(root, &db_path, None, &args, OutputFormat::Human)
                .expect("full sync should succeed from frozen snapshot")
        });

        assert_eq!(exit_code, 0, "full sync should exit successfully");
        assert_source_loaded(root, &db_path, "guide-source");
    }

    #[test]
    fn cmd_sync_workspace_root_source_ignores_stale_v4_migration_snapshots() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let live_file = root.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nLive content.\n"),
        )
        .expect("write live guide.md");

        let stale_snapshot_file = root
            .join(".graphtor")
            .join("data")
            .join("v4-migration-snapshots")
            .join("stale")
            .join("source-0")
            .join("guide.md");
        fs::create_dir_all(
            stale_snapshot_file
                .parent()
                .expect("stale snapshot file should have a parent"),
        )
        .expect("create stale snapshot dir");
        fs::write(
            &stale_snapshot_file,
            docline_md("guide.md", "Guide Snapshot", "# Guide\n\nStale snapshot.\n"),
        )
        .expect("write stale snapshot guide.md");

        write_single_source_config(root, "workspace-source", root);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let full_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: false, // must not use --no-embed during v4 migration rebuild
            data_root: None,
            full: true,
            metrics: false,
            force: false,
        };

        let full_exit_code = cmd_sync(root, &db_path, None, &full_args, OutputFormat::Human)
            .expect("full sync should ignore stale snapshot markdown files");
        assert_eq!(
            full_exit_code, 0,
            "full sync should succeed when stale migration snapshots exist under the workspace data root"
        );
        assert_source_loaded(root, &db_path, "workspace-source");

        let incremental_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let incremental_exit_code =
            cmd_sync(root, &db_path, None, &incremental_args, OutputFormat::Human)
                .expect("incremental sync should ignore stale snapshot markdown files");
        assert_eq!(
            incremental_exit_code, 0,
            "follow-up incremental sync should not re-surface stale snapshot markdown files as live source input"
        );
    }

    #[test]
    fn cmd_sync_full_v4_migration_persists_state_for_followup_incremental_delete() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md(
                "canonical/guide.md",
                "Guide",
                "# Guide\n\nFrozen content.\n",
            ),
        )
        .expect("write guide.md");
        let live_mtime = unix_mtime_secs(&live_file).expect("read live mtime");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let full_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: false, // must not use --no-embed during v4 migration rebuild
            data_root: None,
            full: true,
            metrics: false,
            force: false,
        };

        let full_exit_code = super::v4_prepare_test_hook::with_paths(vec![live_file], || {
            cmd_sync(root, &db_path, None, &full_args, OutputFormat::Human)
                .expect("full sync should succeed from frozen snapshot")
        });

        assert_eq!(full_exit_code, 0, "full sync should exit successfully");

        {
            let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
            let chunks = list_chunks_for_source(&store, "guide-source").expect("list chunks");
            assert!(
                chunks
                    .iter()
                    .any(|chunk| chunk.path == "canonical/guide.md"),
                "full sync should load frozen chunks under the contract source_path: {chunks:?}"
            );
        }

        let state = SyncState::load(&sync_state_path(&db_path), root).expect("load sync state");
        let source_state = state
            .source("guide-source")
            .expect("guide-source state should exist after full migration rebuild");
        assert_eq!(
            source_state.file_mtimes.get("guide.md"),
            Some(&live_mtime),
            "full rebuild must persist the frozen live-source mtime for follow-up incremental syncs"
        );
        assert_eq!(
            source_state
                .file_contract_paths
                .get("guide.md")
                .map(String::as_str),
            Some("canonical/guide.md"),
            "full rebuild must persist the contract source_path for delete cleanup"
        );

        let incremental_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let incremental_exit_code =
            cmd_sync(root, &db_path, None, &incremental_args, OutputFormat::Human)
                .expect("follow-up incremental sync should succeed");

        assert_eq!(
            incremental_exit_code, 0,
            "follow-up incremental sync should exit successfully"
        );

        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        let after =
            list_chunks_for_source(&store, "guide-source").expect("list chunks after delete");
        assert!(
            after.is_empty(),
            "follow-up incremental sync should delete frozen docs that disappeared from the live source: {after:?}"
        );
    }

    #[test]
    fn cmd_sync_incremental_v4_migration_uses_frozen_snapshot_and_live_mtimes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nFrozen content.\n"),
        )
        .expect("write guide.md");
        let live_mtime = unix_mtime_secs(&live_file).expect("read live mtime");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: false, // must not use --no-embed during v4 migration rebuild
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let exit_code = super::v4_prepare_test_hook::with_paths(vec![live_file], || {
            cmd_sync(root, &db_path, None, &args, OutputFormat::Human)
                .expect("incremental sync should succeed from frozen snapshot")
        });

        assert_eq!(exit_code, 0, "incremental sync should exit successfully");
        assert_source_loaded(root, &db_path, "guide-source");

        let state = SyncState::load(&sync_state_path(&db_path), root).expect("load sync state");
        let source_state = state
            .source("guide-source")
            .expect("guide-source state should exist");
        assert_eq!(
            source_state.file_mtimes.get("guide.md"),
            Some(&live_mtime),
            "sync state should retain the live source mtime captured before prune"
        );
    }

    #[test]
    fn cmd_prewarm_v4_migration_gated_retry_ignores_live_acquisition_failure() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nFrozen content.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let source_config = load_source_config(root, &db_path, None)
            .expect("load source config")
            .expect("source config should exist");
        let data_root = root.join(".graphtor").join("data");
        let plan =
            acquire_plan::plan(&source_config, &data_root, root).expect("build acquisition plan");
        let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
        let prepared = prepare_v4_migration_if_needed(&store, plan)
            .expect("initial v4 migration prepare should succeed");
        finalize_v4_migration_if_clean(&store, prepared.migration_started, 1)
            .expect("errorful rebuild should leave the migration gated");
        drop(prepared);

        fs::remove_dir_all(&docs_dir).expect("remove live source after snapshot persists");

        let args = cli::prewarm::PrewarmArgs {
            no_embed: false, // guard rejects --no-embed on pre-v4 DB; test frozen-retry behavior
            data_root: None,
            quiet: true,
        };
        let exit_code = cmd_prewarm(root, &db_path, None, &args)
            .expect("prewarm should rebuild from the persisted snapshot");

        assert_eq!(
            exit_code, 0,
            "prewarm should report success when the prepared frozen retry plan succeeds"
        );
        assert_source_loaded(root, &db_path, "guide-source");
    }

    #[test]
    fn cmd_prewarm_v4_migration_uses_frozen_snapshot_after_live_source_removed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nFrozen content.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::prewarm::PrewarmArgs {
            no_embed: false, // guard rejects --no-embed on pre-v4 DB; test frozen-snapshot behavior
            data_root: None,
            quiet: true,
        };

        let exit_code = super::v4_prepare_test_hook::with_paths(vec![live_file], || {
            cmd_prewarm(root, &db_path, None, &args)
                .expect("prewarm should succeed from frozen snapshot")
        });

        assert_eq!(exit_code, 0, "prewarm should exit successfully");
        assert_source_loaded(root, &db_path, "guide-source");
    }

    // ── Issue 2: --no-embed + v4 migration must be rejected before destructive prune ──

    /// `--no-embed` must be rejected when the database requires a v4 migration
    /// rebuild (full path).  The guard must fire BEFORE `prepare_v4_migration_if_needed`
    /// would prune the database so existing data is preserved.
    #[test]
    fn cmd_sync_full_v4_migration_rejects_no_embed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: true,
            metrics: false,
            force: false,
        };

        let result = cmd_sync(root, &db_path, None, &args, OutputFormat::Human);
        assert!(
            result.is_err(),
            "--no-embed with v4 migration full sync must return Err, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--no-embed"),
            "error must mention --no-embed: {msg}"
        );
        assert!(
            msg.contains("v4 migration"),
            "error must mention v4 migration: {msg}"
        );

        // The database must NOT have been pruned: the pre-v4 gate must still be active.
        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        assert!(
            store.needs_v4_migration().expect("check migration gate"),
            "pre-v4 gate must remain active after rejected --no-embed run"
        );
    }

    /// `--no-embed` must be rejected when the database requires a v4 migration
    /// rebuild (incremental path).
    #[test]
    fn cmd_sync_incremental_v4_migration_rejects_no_embed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let result = cmd_sync(root, &db_path, None, &args, OutputFormat::Human);
        assert!(
            result.is_err(),
            "--no-embed with v4 migration incremental sync must return Err, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--no-embed"),
            "error must mention --no-embed: {msg}"
        );
        // Database must remain intact.
        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        assert!(
            store.needs_v4_migration().expect("check migration gate"),
            "pre-v4 gate must remain active after rejected --no-embed incremental run"
        );
    }

    // ── Issue 2 (prewarm): --no-embed + v4 migration must be rejected before destructive prune ──

    /// `prewarm --no-embed` must be rejected when the database requires a v4
    /// migration rebuild.  The guard must fire BEFORE `prepare_v4_migration_if_needed`
    /// would prune the database so existing data is preserved.
    #[test]
    fn cmd_prewarm_v4_migration_rejects_no_embed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::prewarm::PrewarmArgs {
            no_embed: true,
            data_root: None,
            quiet: true,
        };

        let result = cmd_prewarm(root, &db_path, None, &args);
        assert!(
            result.is_err(),
            "--no-embed with v4 migration prewarm must return Err, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--no-embed"),
            "error must mention --no-embed: {msg}"
        );
        assert!(
            msg.contains("v4 migration"),
            "error must mention v4 migration: {msg}"
        );

        // The database must NOT have been pruned: the pre-v4 gate must still be active.
        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        assert!(
            store.needs_v4_migration().expect("check migration gate"),
            "pre-v4 gate must remain active after rejected --no-embed prewarm"
        );
        // Pre-v4 source data must survive the rejected run.
        let sources = graphtor_core::db::list_sources(&store).expect("list sources");
        assert_eq!(
            sources.len(),
            1,
            "pre-v4 source data must survive rejected --no-embed prewarm; \
             expected 1 source, got {}",
            sources.len()
        );
    }

    /// `prewarm --no-embed` on a v4 database must succeed: the guard only fires
    /// when a destructive rebuild is imminent.
    #[test]
    fn cmd_prewarm_no_embed_accepted_on_v4_database() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        // Use a fresh (v4) database — no migration needed.
        let db_path = root.join("graph.db");
        let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
        store.ensure_schema().expect("ensure schema");
        drop(store);

        let args = cli::prewarm::PrewarmArgs {
            no_embed: true,
            data_root: None,
            quiet: true,
        };

        let result = cmd_prewarm(root, &db_path, None, &args);
        assert!(
            result.is_ok(),
            "--no-embed must be accepted on a v4 database; got: {result:?}"
        );
        assert_eq!(
            result.unwrap(),
            0,
            "prewarm --no-embed on v4 database must exit successfully"
        );
    }

    // ── Issue 2b: --no-embed + epoch-mismatch rebuild must be rejected ────────

    /// Helper: write sync state with a stale contract epoch for `source_id`.
    ///
    /// Simulates a source previously synced under an old contract epoch, so the
    /// next sync would detect an epoch mismatch and force a full re-ingest.
    fn seed_stale_epoch_sync_state(root: &Path, db_path: &Path, source_id: &str) {
        let mut state = SyncState::default();
        let src = state.source_mut(source_id);
        src.file_mtimes
            .insert("guide.md".to_string(), 1_700_000_000u64);
        src.last_sync = Some("2026-01-01T00:00:00Z".to_string());
        // Use an epoch string that is not the current CONTRACT_EPOCH to trigger
        // the epoch-mismatch forced-rebuild path in sync_source.
        src.contract_epoch = Some("stale-epoch-0".to_string());
        state
            .save(&sync_state_path(db_path), root)
            .expect("seed stale epoch sync state");
    }

    /// `sync --no-embed` must be rejected when a stored epoch mismatch would
    /// force a full re-ingest, before any destructive rebuild begins.
    #[test]
    fn cmd_sync_epoch_mismatch_rejects_no_embed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        // v4 database (no migration needed) — the trigger is the epoch mismatch.
        let db_path = root.join("graph.db");
        {
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            store.ensure_schema().expect("ensure schema");
        }

        // Seed sync state with a stale epoch so the next sync detects a mismatch.
        seed_stale_epoch_sync_state(root, &db_path, "guide-source");

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let result = cmd_sync(root, &db_path, None, &args, OutputFormat::Human);
        assert!(
            result.is_err(),
            "sync --no-embed with epoch mismatch must return Err, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--no-embed"),
            "error must mention --no-embed: {msg}"
        );
        assert!(msg.contains("epoch"), "error must mention epoch: {msg}");
    }

    /// `prewarm --no-embed` must be rejected when a stored epoch mismatch would
    /// force a full re-ingest, before any destructive rebuild begins.
    #[test]
    fn cmd_prewarm_epoch_mismatch_rejects_no_embed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        fs::write(
            docs_dir.join("guide.md"),
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        // v4 database (no migration needed) — the trigger is the epoch mismatch.
        let db_path = root.join("graph.db");
        {
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            store.ensure_schema().expect("ensure schema");
        }

        // Seed sync state with a stale epoch so the next sync detects a mismatch.
        seed_stale_epoch_sync_state(root, &db_path, "guide-source");

        let args = cli::prewarm::PrewarmArgs {
            no_embed: true,
            data_root: None,
            quiet: true,
        };

        let result = cmd_prewarm(root, &db_path, None, &args);
        assert!(
            result.is_err(),
            "prewarm --no-embed with epoch mismatch must return Err, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--no-embed"),
            "error must mention --no-embed: {msg}"
        );
        assert!(msg.contains("epoch"), "error must mention epoch: {msg}");
    }

    /// `sync --no-embed` must be accepted when sync state carries the current
    /// contract epoch — no epoch-mismatch rebuild is needed.
    #[test]
    fn cmd_sync_no_embed_accepted_when_epoch_current() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nContent.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        // v4 database.
        let db_path = root.join("graph.db");
        {
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            store.ensure_schema().expect("ensure schema");
        }

        // Seed sync state with the CURRENT epoch — no mismatch.
        seed_current_sync_state(root, &db_path, "guide-source", "guide.md", 1_700_000_000);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let result = cmd_sync(root, &db_path, None, &args, OutputFormat::Human);
        assert!(
            result.is_ok(),
            "sync --no-embed with current epoch must succeed; got: {result:?}"
        );
        assert_eq!(
            result.unwrap(),
            0,
            "sync --no-embed with current epoch must exit with code 0"
        );
    }

    // ── Issue 3: full sync (non-migration) must seed sync state ──────────────

    /// After a regular `--full` sync (no v4 migration), sync state must be
    /// seeded from the live source files so the next incremental cycle does not
    /// re-ingest every file from scratch.
    #[test]
    fn cmd_sync_full_regular_seeds_sync_state_for_incremental_followup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("docs/guide.md", "Guide", "# Guide\n\nStable content.\n"),
        )
        .expect("write guide.md");
        write_single_source_config(root, "guide-source", &docs_dir);

        let db_path = root.join("graph.db");
        // Use a fresh (non-v3) database so no v4 migration is needed.
        {
            let store = DataStore::open_sqlite(&db_path, root).expect("open sqlite store");
            store.ensure_schema().expect("ensure schema");
        }

        let full_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: true,
            metrics: false,
            force: false,
        };

        let full_exit_code = cmd_sync(root, &db_path, None, &full_args, OutputFormat::Human)
            .expect("full sync should succeed");
        assert_eq!(full_exit_code, 0, "full sync should exit successfully");

        // Sync state must be seeded after the full sync.
        let state = SyncState::load(&sync_state_path(&db_path), root).expect("load sync state");
        let source_state = state
            .source("guide-source")
            .expect("guide-source state must exist after full sync");
        assert!(
            source_state.file_mtimes.contains_key("guide.md"),
            "sync state must record the live mtime for guide.md after full sync: {source_state:?}"
        );
        assert_eq!(
            source_state
                .file_contract_paths
                .get("guide.md")
                .map(String::as_str),
            Some("docs/guide.md"),
            "sync state must record the contract source_path for guide.md after full sync"
        );
        assert_eq!(
            source_state.contract_epoch.as_deref(),
            Some(graphtor_core::ingest_contract::CONTRACT_EPOCH),
            "sync state must carry the current contract epoch after full sync"
        );

        // The follow-up incremental sync must detect no changes.
        let incremental_args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let incremental_exit_code =
            cmd_sync(root, &db_path, None, &incremental_args, OutputFormat::Human)
                .expect("follow-up incremental sync should succeed");
        assert_eq!(
            incremental_exit_code, 0,
            "follow-up incremental sync should exit successfully"
        );

        // After the incremental sync, the chunks count should equal the full sync count
        // (no duplication, no re-ingestion since files did not change).
        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        let chunks =
            graphtor_core::db::list_chunks_for_source(&store, "guide-source").expect("list chunks");
        assert!(
            !chunks.is_empty(),
            "chunks must remain after follow-up incremental sync"
        );
    }

    #[test]
    fn cmd_sync_empty_registry_completes_staged_empty_v4_migration() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_empty_source_config(root);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let exit_code = cmd_sync(root, &db_path, None, &args, OutputFormat::Human)
            .expect("sync should complete the staged empty-candidate migration");

        assert_eq!(exit_code, 0, "sync should exit successfully");

        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        assert!(
            !store
                .needs_v4_migration()
                .expect("check cleared migration gate"),
            "empty-registry sync must clear the v4 migration gate"
        );
        assert!(
            graphtor_core::db::list_sources(&store)
                .expect("list sources after empty-registry sync")
                .is_empty(),
            "empty-registry sync must leave the rebuilt database empty"
        );
    }

    #[test]
    fn empty_registry_v4_migration_clears_sync_state_for_restored_sources() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs_dir = root.join("docs");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        let live_file = docs_dir.join("guide.md");
        fs::write(
            &live_file,
            docline_md("guide.md", "Guide", "# Guide\n\nRestored content.\n"),
        )
        .expect("write guide.md");
        let live_mtime = unix_mtime_secs(&live_file).expect("read live mtime");

        let db_path = root.join("graph.db");
        seed_store_at_v3_with_source(&db_path, root, "guide-source");
        seed_current_sync_state(root, &db_path, "guide-source", "guide.md", live_mtime);

        let data_root = root.join(".graphtor").join("data");
        assert!(
            complete_empty_registry_v4_migration_if_needed(root, &db_path, &data_root)
                .expect("empty-registry migration should succeed"),
            "pre-v4 database should run the empty-registry migration"
        );

        let state = SyncState::load(&sync_state_path(&db_path), root).expect("load sync state");
        assert!(
            state.source("guide-source").is_none(),
            "empty-registry migration must clear stale sync state so restored sources re-ingest"
        );

        write_single_source_config(root, "guide-source", &docs_dir);
        let args = cli::SyncArgs {
            batch_size: 20,
            no_embed: true,
            data_root: None,
            full: false,
            metrics: false,
            force: false,
        };

        let exit_code = cmd_sync(root, &db_path, None, &args, OutputFormat::Human)
            .expect("follow-up incremental sync should succeed");
        assert_eq!(
            exit_code, 0,
            "follow-up incremental sync should re-ingest restored sources"
        );
        assert_source_loaded(root, &db_path, "guide-source");
    }

    #[test]
    fn cmd_prewarm_empty_registry_completes_staged_empty_v4_migration() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_empty_source_config(root);

        let db_path = root.join("graph.db");
        seed_store_at_v3(&db_path, root);

        let args = cli::prewarm::PrewarmArgs {
            no_embed: true,
            data_root: None,
            quiet: true,
        };

        let exit_code = cmd_prewarm(root, &db_path, None, &args)
            .expect("prewarm should complete the staged empty-candidate migration");

        assert_eq!(exit_code, 0, "prewarm should exit successfully");

        let store = DataStore::open_sqlite(&db_path, root).expect("reopen sqlite store");
        assert!(
            !store
                .needs_v4_migration()
                .expect("check cleared migration gate"),
            "empty-registry prewarm must clear the v4 migration gate"
        );
        assert!(
            graphtor_core::db::list_sources(&store)
                .expect("list sources after empty-registry prewarm")
                .is_empty(),
            "empty-registry prewarm must leave the rebuilt database empty"
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
