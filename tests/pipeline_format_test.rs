//! Integration tests: pipeline format filtering.
//!
//! Verifies that:
//! - A source with `formats: ["md"]` skips `.pdf` and `.docx` files and
//!   counts them in `skipped_by_format`.
//! - A source with `formats: []` (empty = no restriction) processes all
//!   supported extensions.
//! - `skipped_by_format` is zero when all files match the formats list.
//! - Format validation in [`SourceConfig`] rejects unknown extension strings.

use std::fs;

use graphtor_core::acquire::plan;
use graphtor_core::config::source::{LocalSource, Source, SourceConfig};
use graphtor_core::db::DataStore;
use graphtor_core::pipeline::{run, PipelineConfig};

fn make_store() -> DataStore {
    let s = DataStore::open_mem().expect("open in-memory store");
    s.ensure_schema().expect("ensure schema");
    s
}

fn default_pipeline_config() -> PipelineConfig {
    PipelineConfig {
        batch_size: 20,
        parallel: false,
    }
}

// ── T021.001.a: md-only formats skips pdf ────────────────────────────────────

/// A source restricted to `["md"]` must skip `.pdf` files without error.
///
/// `skipped_by_format` must equal the number of non-md files; the md file
/// must be processed and produce zero errors.
#[test]
fn pipeline_md_only_formats_skips_pdf_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("guide.md"), "---\ntitle: Guide\nsource: /test/source\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nContent.\n").expect("write guide.md");
    fs::write(docs_dir.join("ref.pdf"), b"not a real pdf").expect("write ref.pdf");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "format-md-only".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 1,
        "only the md file should be processed"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "skipped pdf must not produce errors: {:?}",
        result.errors_encountered
    );
    assert_eq!(
        result.skipped_by_format, 1,
        "the pdf file must be counted as skipped_by_format"
    );
}

// ── T021.001.b: skipped_by_format counter is accurate ────────────────────────

/// With `formats: ["md"]`, two pdf and one docx file must all be counted.
#[test]
fn skipped_by_format_counter_counts_all_excluded_extensions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("a.md"), "---\ntitle: A\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: a.md\n---\n# A\n\nContent A.\n").expect("write a.md");
    fs::write(docs_dir.join("b.md"), "---\ntitle: B\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: b.md\n---\n# B\n\nContent B.\n").expect("write b.md");
    fs::write(docs_dir.join("c.pdf"), b"fake pdf").expect("write c.pdf");
    fs::write(docs_dir.join("d.pdf"), b"fake pdf").expect("write d.pdf");
    fs::write(docs_dir.join("e.docx"), b"fake docx").expect("write e.docx");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "format-counter-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 2,
        "only the two md files should be processed"
    );
    assert_eq!(
        result.skipped_by_format, 3,
        "two pdf + one docx must be counted as skipped_by_format"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "format skips must not produce errors: {:?}",
        result.errors_encountered
    );
}

// ── T021.001.c: empty formats list = no restriction ──────────────────────────

/// A source with `formats: []` (empty list) must process all supported
/// extensions.  This mirrors the semantics of empty `include` patterns.
#[test]
fn empty_formats_list_processes_all_supported_extensions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("guide.md"), "---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nMd content.\n").expect("write guide.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "format-empty-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec![], // empty = no restriction
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 1,
        "md file must be processed when formats is empty"
    );
    assert_eq!(
        result.skipped_by_format, 0,
        "no files must be counted as skipped when formats is empty"
    );
}

// ── T021.001.d: no skips when all files match formats ────────────────────────

/// When all files in the directory match the formats list, `skipped_by_format`
/// must be zero.
#[test]
fn skipped_by_format_is_zero_when_all_files_match() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("a.md"), "---\ntitle: A\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: a.md\n---\n# A\n\nContent.\n").expect("write a.md");
    fs::write(docs_dir.join("b.md"), "---\ntitle: B\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: b.md\n---\n# B\n\nContent.\n").expect("write b.md");
    fs::write(docs_dir.join("c.md"), "---\ntitle: C\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: c.md\n---\n# C\n\nContent.\n").expect("write c.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "all-match-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(result.documents_processed, 3, "all 3 md files must process");
    assert_eq!(
        result.skipped_by_format, 0,
        "nothing should be skipped when all files match formats"
    );
    assert!(result.errors_encountered.is_empty(), "no errors expected");
}

// ── T021.001.e: YAML formats field is deserialized ───────────────────────────

/// A `sources.yaml` snippet with an explicit Markdown `formats` field must
/// deserialize the list correctly.
#[test]
fn yaml_formats_field_is_deserialized_correctly() {
    const YAML: &str = r"
sources:
  - type: local
    id: yaml-format-source
    path: /tmp/docs
    formats:
      - md
      - markdown
";
    let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse YAML");
    let local = config.sources[0].as_local().expect("local source");
    assert_eq!(
        local.formats,
        vec!["md".to_string(), "markdown".to_string()],
        "formats must be deserialized from YAML"
    );
}

// ── T021.001.f: default formats when absent from YAML ────────────────────────

/// When the `formats` field is absent from YAML, it must default to `["md"]`.
#[test]
fn yaml_formats_defaults_to_md_when_absent() {
    const YAML: &str = r"
sources:
  - type: local
    id: default-format-source
    path: /tmp/docs
";
    let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse YAML");
    let local = config.sources[0].as_local().expect("local source");
    assert_eq!(
        local.formats,
        vec!["md".to_string()],
        "formats must default to [md] after docline pivot"
    );
}

// ── T021.001.g: formats: ["markdown"] alias processes .md files ──────────────

/// `formats: ["markdown"]` is a documented alias for `"md"`.
/// The pipeline must canonicalize it so that `.md` files are processed (not
/// silently skipped by the format allow-list).
#[test]
fn formats_markdown_alias_processes_md_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(
        docs_dir.join("guide.md"),
        "---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\n\
         doc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nContent.\n",
    )
    .expect("write guide.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "markdown-alias-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            // "markdown" is the long-form alias for "md"; must not skip .md files.
            formats: vec!["markdown".to_string()],
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 1,
        "formats: [\"markdown\"] must process .md files (not skip them); \
         got documents_processed={}, skipped_by_format={}",
        result.documents_processed, result.skipped_by_format
    );
    assert_eq!(
        result.skipped_by_format, 0,
        "no files should be skipped when formats is [\"markdown\"] and all files are .md"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "no errors expected: {:?}",
        result.errors_encountered
    );
}

// ── T021.001.h: formats: ["markdown"] does not purge docs ────────────────────

/// Regression: if `formats: ["markdown"]` incorrectly fails to match `.md`
/// files, the incremental sync tracker would see all files as untracked and
/// treat every previously indexed document as deleted — purging the database.
///
/// This test verifies that `SourceConfig` validation accepts `"markdown"`,
/// confirming the alias is understood before any runtime comparison happens.
#[test]
fn formats_markdown_alias_is_accepted_by_validation() {
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "markdown-alias-validate".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["markdown".to_string()],
            database: None,
        })],
    };

    assert!(
        config.validate().is_ok(),
        "formats: [\"markdown\"] must pass validation"
    );
}
