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
#      flags present in each specific step's own *executable* command text
#      -- extracted from the `run:` value only, with any comment lines
#      stripped out, so a stale descriptive comment mentioning the same
#      flags can never satisfy the check on its own.
#   2. It FAILS CLOSED on a seeded clippy::pedantic violation, using the
#      clippy step's *actual* extracted `run:` command (manifest path
#      substituted to point at the scratch copy) rather than a separately
#      hand-maintained mirror of its flags -- so the CI job's own command
#      text is what is proved, with nothing left to drift out of sync.
#   3. The disposable scratch copy used to seed that violation is never
#      built from a probe tree containing a symlink (fail-closed guard
#      before the tar archive is created).
#
# Never edits tools/mcp-probe/ in place -- CI/workflow width only (056.028-T
# does not own probe source). All seeding happens in a disposable temp copy.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROBE_DIR="$REPO_ROOT/tools/mcp-probe"
WORKFLOW_FILE="$REPO_ROOT/.github/workflows/mcp-probe-ci.yml"
REAL_MANIFEST_REL="tools/mcp-probe/Cargo.toml"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# Prints only the *executable* text of the named workflow step's `run:`
# value (single-line `run: <cmd>` or multi-line `run: |` block scalar),
# with comment lines (leading `#`, after left-trim) filtered out and each
# line's shared block-scalar indentation stripped. Never returns any text
# from outside the `run:` key itself (in particular, never the step's own
# preceding descriptive `#` comments, which live above `run:` at the same
# indentation and are excluded by construction, not merely by a
# post-hoc comment filter). Step names used here (clippy/test/build/audit)
# are unique to the probe-ci job, and this workflow deliberately keeps
# every dependency-resolving command on `run:` lines only (mawk-compatible;
# uses no gawk-only extensions since this must also run unmodified under
# ubuntu-latest's default /usr/bin/awk).
extract_run_body() {
  local step_name="$1"
  awk -v marker="- name: ${step_name}" '
    BEGIN { capture=0; in_run=0; run_indent=-1; done=0 }
    !done && !capture && $0 ~ ("^[[:space:]]*" marker "[[:space:]]*$") { capture=1; next }
    !done && capture && !in_run && $0 ~ /^[[:space:]]*- name:/ { capture=0; done=1; next }
    !done && capture && !in_run && $0 ~ /^  [A-Za-z0-9_-]+:/ { capture=0; done=1; next }
    !done && capture && !in_run && $0 ~ /^[[:space:]]*run:[[:space:]]*\|[[:space:]]*$/ {
      in_run=1
      line=$0
      sub(/run:.*/, "", line)
      run_indent=length(line)
      next
    }
    !done && capture && !in_run && $0 ~ /^[[:space:]]*run:[[:space:]]*/ {
      val=$0
      sub(/^[[:space:]]*run:[[:space:]]*/, "", val)
      if (val !~ /^[[:space:]]*#/) print val
      capture=0
      done=1
      next
    }
    !done && in_run {
      if ($0 ~ /^[[:space:]]*$/) { next }
      line=$0
      sub(/[^ ].*$/, "", line)
      cur_indent=length(line)
      if (cur_indent <= run_indent) { in_run=0; capture=0; done=1; next }
      trimmed=$0
      sub(/^[[:space:]]*/, "", trimmed)
      if (trimmed !~ /^#/) print trimmed
    }
  ' "$WORKFLOW_FILE"
}

assert_run_body_contains() {
  local step_name="$1" needle="$2"
  local body
  body="$(extract_run_body "$step_name")"
  [ -n "$body" ] || fail "workflow step '$step_name' run: value not found"
  echo "$body" | grep -qF -- "$needle" \
    || fail "workflow step '$step_name' run: command does not contain expected: $needle"
}

echo "== Structural check: probe-ci steps' actual run: commands bind the standalone manifest/lockfile, --locked, and lint/audit flags =="
[ -f "$WORKFLOW_FILE" ] || fail "workflow file not found: $WORKFLOW_FILE"
assert_run_body_contains "clippy" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_run_body_contains "clippy" "--locked"
assert_run_body_contains "clippy" "-D warnings"
assert_run_body_contains "clippy" "-D clippy::pedantic"
assert_run_body_contains "test" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_run_body_contains "test" "--locked"
assert_run_body_contains "build" "--manifest-path tools/mcp-probe/Cargo.toml"
assert_run_body_contains "build" "--locked"
assert_run_body_contains "audit" "--file tools/mcp-probe/Cargo.lock"
assert_run_body_contains "audit" "--deny warnings"
echo "OK: probe-ci's clippy/test/build/audit run: commands are bound to the standalone manifest/lockfile, --locked, and the required lint/audit flags (comments excluded from the check)."

CLIPPY_RUN_BODY="$(extract_run_body "clippy")"
[ -n "$CLIPPY_RUN_BODY" ] || fail "could not extract the clippy step's run: command from the workflow"

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

echo "== RED proof: the workflow's own extracted clippy command, pointed at the seeded copy, must fail =="
# Substitutes only the manifest path so the executed command is otherwise
# byte-identical to what probe-ci actually runs -- this is "share one
# executable command with the workflow" rather than a separately
# maintained mirror of its flags. Substitutes the bare relative
# "Cargo.toml" (executed with cwd=$SEED_COPY) rather than an absolute
# $SCRATCH_DIR-rooted path: mirrors the .ps1 twin's defense-in-depth fix
# for TEMP/profile paths that can contain spaces on Windows -- a lower-risk
# concern on the Linux CI runner this script actually targets (TMPDIR is
# always /tmp there), but keeping both twins' seeded invocations identically
# shaped removes even the theoretical risk and any drift between them.
SEEDED_RUN_BODY="${CLIPPY_RUN_BODY//$REAL_MANIFEST_REL/Cargo.toml}"
if (cd "$SEED_COPY" && bash -c "$SEEDED_RUN_BODY") >"$SCRATCH_DIR/seeded-clippy.log" 2>&1; then
  cat "$SCRATCH_DIR/seeded-clippy.log" >&2
  fail "seeded violation did not fail clippy -- the CI job would not have caught it"
fi
grep -q 'must_use_candidate' "$SCRATCH_DIR/seeded-clippy.log" \
  || fail "clippy failed, but not for the seeded must_use_candidate violation -- check for an unrelated regression"
echo "OK: seeded clippy::pedantic violation is caught (non-zero exit, must_use_candidate reported) by the workflow's own extracted command."

echo "== GREEN proof: the workflow's own extracted clippy command passes against the real, unmodified crate =="
(cd "$REPO_ROOT" && bash -c "$CLIPPY_RUN_BODY") \
  || fail "the real tools/mcp-probe crate unexpectedly failed clippy -- investigate before wiring this job into required checks"
echo "OK: the real tools/mcp-probe crate passes the probe-ci clippy gate (using the workflow's own extracted command, executed unmodified)."

echo "PASS: mcp-probe CI seeded-violation self-test complete."
