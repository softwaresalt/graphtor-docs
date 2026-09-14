#!/usr/bin/env bash
# mcp-probe-ci-seeded-violation-check.sh
#
# Test-first dry-run proof for 056.028-T's dedicated mcp-probe CI job
# (.github/workflows/mcp-probe-ci.yml). Proves three things about the
# `probe-ci` job before/independent of it being wired into required checks:
#
#   1. Its clippy/test/build/audit steps are actually bound to the
#      standalone tools/mcp-probe/Cargo.toml / Cargo.lock manifest+lockfile
#      (not the root workspace), with --locked and the required lint/audit
#      flags present in each specific step's own command block (not merely
#      referenced somewhere in the file).
#   2. It FAILS CLOSED on a seeded clippy::pedantic violation, rather than
#      always reporting green regardless of content.
#   3. The disposable scratch copy used to seed that violation is never
#      built from a probe tree containing a symlink (fail-closed guard
#      before the tar archive is created).
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
CLIPPY_ARGS=(--locked --all-targets -- -D warnings -D clippy::pedantic -A clippy::module_name_repetitions)

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# Prints the lines belonging to the named workflow step (from its
# `- name: <step>` marker, exclusive, up to but excluding the next
# `- name:` line), so structural assertions below are bound to that exact
# step's command instead of matching text anywhere in the file. Step names
# used here (clippy/test/build/audit) are unique to the probe-ci job.
extract_step() {
  local step_name="$1"
  awk -v marker="- name: ${step_name}" '
    $0 ~ ("^[[:space:]]*" marker "[[:space:]]*$") { capture=1; next }
    capture && $0 ~ /^[[:space:]]*- name:/ { capture=0 }
    capture && $0 ~ /^  [A-Za-z0-9_-]+:/ { capture=0 }
    capture { print }
  ' "$WORKFLOW_FILE"
}

assert_step_contains() {
  local step_name="$1" needle="$2"
  local block
  block="$(extract_step "$step_name")"
  [ -n "$block" ] || fail "workflow step '$step_name' not found"
  echo "$block" | grep -qF -- "$needle" \
    || fail "workflow step '$step_name' does not contain expected: $needle"
}

echo "== Structural check: probe-ci steps bind the standalone manifest/lockfile, --locked, and lint/audit flags =="
[ -f "$WORKFLOW_FILE" ] || fail "workflow file not found: $WORKFLOW_FILE"
assert_step_contains "clippy" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_step_contains "clippy" "--locked"
assert_step_contains "clippy" "-D warnings"
assert_step_contains "clippy" "-D clippy::pedantic"
assert_step_contains "test" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_step_contains "test" "--locked"
assert_step_contains "build" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_step_contains "build" "--locked"
assert_step_contains "audit" "--file tools/mcp-probe/Cargo.lock"
assert_step_contains "audit" "--deny warnings"
echo "OK: probe-ci's clippy/test/build/audit steps are bound to the standalone manifest/lockfile, --locked, and the required lint/audit flags."

SCRATCH_DIR="$(mktemp -d)"
cleanup() { rm -rf "$SCRATCH_DIR"; }
trap cleanup EXIT

echo "== Symlink safety check: refuse to seed if the probe tree contains a symlink =="
# tar (below) archives symlinks as links, and a symlink at e.g. src/lib.rs
# pointing outside the crate would make the later `cat >>` append write
# through it -- a path-traversal risk if such a symlink were ever
# introduced (accidentally or via a crafted PR). Fail closed rather than
# dereferencing or otherwise trying to sanitize it. Excludes ./target to
# match the tar --exclude below (build output, not source).
symlinks="$(find "$PROBE_DIR" -path "$PROBE_DIR/target" -prune -o -type l -print)"
if [ -n "$symlinks" ]; then
  echo "$symlinks" >&2
  fail "tools/mcp-probe contains one or more symlinks -- refusing to seed a scratch copy (potential path traversal via tar/append)"
fi
echo "OK: no symlinks found under tools/mcp-probe (excluding target/)."

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
