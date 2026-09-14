#!/usr/bin/env bash
# mcp-probe-ci-seeded-violation-check.sh
#
# Test-first dry-run proof for 056.028-T's dedicated mcp-probe CI job
# (.github/workflows/mcp-probe-ci.yml). Proves two things about the
# `probe-ci` job's clippy gate before/independent of it being wired into
# required checks:
#
#   1. It actually SELECTS the standalone tools/mcp-probe/Cargo.toml /
#      tools/mcp-probe/Cargo.lock manifest+lockfile (not the root workspace).
#   2. It FAILS CLOSED on a seeded clippy::pedantic violation, rather than
#      always reporting green regardless of content.
#
# Runs the exact clippy invocation used by the `probe-ci` job (including the
# documented `-A clippy::module_name_repetitions` workflow-level allowance)
# against a scratch copy of the crate with one extra seeded violation
# (`clippy::must_use_candidate`, a distinct pedantic lint not covered by that
# allowance), then re-runs the identical command against the real,
# unmodified checked-out crate to prove the current source is clean.
#
# Never edits tools/mcp-probe/ in place -- CI/workflow width only (056.028-T
# does not own probe source). All seeding happens in a disposable temp copy.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROBE_DIR="$REPO_ROOT/tools/mcp-probe"
WORKFLOW_FILE="$REPO_ROOT/.github/workflows/mcp-probe-ci.yml"
TOOLCHAIN="+1.75.0"
CLIPPY_ARGS=(--all-targets -- -D warnings -D clippy::pedantic -A clippy::module_name_repetitions)

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

echo "== Structural check: workflow selects the standalone manifest/lockfile =="
[ -f "$WORKFLOW_FILE" ] || fail "workflow file not found: $WORKFLOW_FILE"
grep -q 'tools/mcp-probe/Cargo.toml' "$WORKFLOW_FILE" \
  || fail "workflow does not reference tools/mcp-probe/Cargo.toml"
grep -q 'tools/mcp-probe/Cargo.lock' "$WORKFLOW_FILE" \
  || fail "workflow does not reference tools/mcp-probe/Cargo.lock"
echo "OK: workflow references the standalone probe manifest and lockfile."

SCRATCH_DIR="$(mktemp -d)"
cleanup() { rm -rf "$SCRATCH_DIR"; }
trap cleanup EXIT

echo "== Seeding a clippy::pedantic violation into a disposable scratch copy =="
SEED_COPY="$SCRATCH_DIR/mcp-probe-seeded"
mkdir -p "$SEED_COPY"
# Copy everything except the build output directory (not needed, and large).
(cd "$PROBE_DIR" && tar --exclude='./target' -cf - .) | (cd "$SEED_COPY" && tar -xf -)

cat >>"$SEED_COPY/src/lib.rs" <<'RUST'

// Seeded by scripts/mcp-probe-ci-seeded-violation-check.sh (test-first red
// proof for 056.028-T). Not part of the real crate -- this file only exists
// in a disposable scratch copy and is discarded after this check runs.
// Triggers clippy::pedantic's `must_use_candidate`: a public, side-effect-free
// function whose result is not `#[must_use]`.
pub fn seeded_violation_probe_must_use_candidate(x: u32) -> u32 {
    x + 1
}
RUST

echo "== RED proof: seeded copy must fail the exact probe-ci clippy invocation =="
if cargo "$TOOLCHAIN" clippy --manifest-path "$SEED_COPY/Cargo.toml" "${CLIPPY_ARGS[@]}" \
    >"$SCRATCH_DIR/seeded-clippy.log" 2>&1; then
  cat "$SCRATCH_DIR/seeded-clippy.log" >&2
  fail "seeded violation did not fail clippy -- the CI job would not have caught it"
fi
grep -q 'must_use_candidate' "$SCRATCH_DIR/seeded-clippy.log" \
  || fail "clippy failed, but not for the seeded must_use_candidate violation -- check for an unrelated regression"
echo "OK: seeded clippy::pedantic violation is caught (non-zero exit, must_use_candidate reported)."

echo "== GREEN proof: the real, unmodified crate passes the identical invocation =="
cargo "$TOOLCHAIN" clippy --manifest-path "$PROBE_DIR/Cargo.toml" "${CLIPPY_ARGS[@]}" \
  || fail "the real tools/mcp-probe crate unexpectedly failed clippy -- investigate before wiring this job into required checks"
echo "OK: the real tools/mcp-probe crate passes the probe-ci clippy gate."

echo "PASS: mcp-probe CI seeded-violation self-test complete."
