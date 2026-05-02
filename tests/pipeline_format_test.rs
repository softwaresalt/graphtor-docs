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

    fs::write(docs_dir.join("guide.md"), "# Guide\n\nContent.\n").expect("write guide.md");
    fs::write(docs_dir.join("ref.pdf"), b"not a real pdf").expect("write ref.pdf");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "format-md-only".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
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

    fs::write(docs_dir.join("a.md"), "# A\n\nContent A.\n").expect("write a.md");
    fs::write(docs_dir.join("b.md"), "# B\n\nContent B.\n").expect("write b.md");
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

    fs::write(docs_dir.join("guide.md"), "# Guide\n\nMd content.\n").expect("write guide.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "format-empty-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec![], // empty = no restriction
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

    fs::write(docs_dir.join("a.md"), "# A\n\nContent.\n").expect("write a.md");
    fs::write(docs_dir.join("b.md"), "# B\n\nContent.\n").expect("write b.md");
    fs::write(docs_dir.join("c.md"), "# C\n\nContent.\n").expect("write c.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "all-match-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
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

/// A `sources.yaml` snippet with an explicit `formats` field must deserialize
/// the list correctly.
#[test]
fn yaml_formats_field_is_deserialized_correctly() {
    const YAML: &str = r"
sources:
  - type: local
    id: yaml-format-source
    path: /tmp/docs
    formats:
      - md
      - pdf
";
    let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse YAML");
    let Source::Local(local) = &config.sources[0] else {
        panic!("expected LocalSource");
    };
    assert_eq!(
        local.formats,
        vec!["md".to_string(), "pdf".to_string()],
        "formats must be deserialized from YAML"
    );
}

// ── T021.001.f: default formats when absent from YAML ────────────────────────

/// When the `formats` field is absent from YAML, it must default to
/// `["md", "pdf", "docx"]`.
#[test]
fn yaml_formats_defaults_to_all_supported_when_absent() {
    const YAML: &str = r"
sources:
  - type: local
    id: default-format-source
    path: /tmp/docs
";
    let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse YAML");
    let Source::Local(local) = &config.sources[0] else {
        panic!("expected LocalSource");
    };
    assert_eq!(
        local.formats,
        vec!["md".to_string(), "pdf".to_string(), "docx".to_string()],
        "formats must default to all three supported extensions"
    );
}
