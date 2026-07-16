//! Integration test locking the consumption-first post-install message
//! contract (P2-T7a): the minimal-install path prints drop-a-db guidance
//! plus a reference to the ingestion-setup docs (P2-T7b), not the
//! ingestion-oriented message the full (`--with-ingestion`) path prints.

use std::process::Command;

fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn default_install_prints_the_consumption_first_post_install_message() {
    let workspace = tempfile::tempdir().expect("tempdir");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("install")
        .output()
        .expect("run graphtor-docs install");
    assert!(
        output.status.success(),
        "install failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(
        stdout.contains("drop a `.db` file into") && stdout.contains("serve it read-only"),
        "consumption-first message must guide the operator to drop a .db file and serve it \
         read-only: {stdout}"
    );
    assert!(
        stdout.contains("--with-ingestion"),
        "consumption-first message must mention the --with-ingestion opt-in: {stdout}"
    );
    assert!(
        stdout.contains("ingestion setup guide")
            && stdout.contains("docs/cli-reference/graphtor-docs.md"),
        "consumption-first message must reference the ingestion-setup docs section: {stdout}"
    );
    assert!(
        !stdout.contains("edit .graphtor/config/sources.yaml"),
        "the minimal-install path must NOT print the ingestion-oriented message: {stdout}"
    );
}

#[test]
fn with_ingestion_install_prints_the_ingestion_oriented_message_not_the_minimal_one() {
    let workspace = tempfile::tempdir().expect("tempdir");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args(["install", "--with-ingestion", "--no-gitignore"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(
        stdout.contains("edit .graphtor/config/sources.yaml to add documentation sources"),
        "the --with-ingestion path must print the ingestion-oriented next steps: {stdout}"
    );
    assert!(
        !stdout.contains("drop a `.db` file into"),
        "the --with-ingestion path must NOT print the consumption-first minimal message: \
         {stdout}"
    );
}
