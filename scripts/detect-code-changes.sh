#!/usr/bin/env bash
# Decide whether the expensive Rust CI pipeline must run for the current event.
#
# Emits `code=true|false` to $GITHUB_OUTPUT. Fail-safe by design: any change that
# is not a known doc/harness path forces a full run, as do scheduled/manual/
# unknown events and any case where the base ref cannot be determined. This runs
# in a lightweight gate job so a CI check is always reported (automation and
# reviewers never see a missing status) while the costly build is skipped for
# changes that touch only non-code files.
#
# Required environment variables (provided by the workflow):
#   EVENT_NAME  github.event_name
#   BASE_SHA    github.event.pull_request.base.sha (pull_request events)
#   BEFORE_SHA  github.event.before                 (push events)
#   HEAD_SHA    github.sha
set -euo pipefail

emit() {
  echo "code=$1" >>"${GITHUB_OUTPUT:-/dev/stdout}"
  echo "run heavy build: $1"
}

case "${EVENT_NAME:-}" in
  pull_request) base="${BASE_SHA:-}" ;;
  push) base="${BEFORE_SHA:-}" ;;
  *)
    echo "event '${EVENT_NAME:-}' is not push/pull_request -> full CI"
    emit true
    exit 0
    ;;
esac

# New branch / missing base (all-zero SHA) -> cannot diff -> run full CI.
if [ -z "$base" ] || [ "$base" = "0000000000000000000000000000000000000000" ]; then
  echo "no usable base ref -> full CI"
  emit true
  exit 0
fi

# Fail-safe: if either endpoint is unreachable (e.g. a rewritten history) or the
# diff itself errors, run the full build rather than risk skipping needed CI.
if ! git cat-file -e "${base}^{commit}" 2>/dev/null ||
  ! git cat-file -e "${HEAD_SHA}^{commit}" 2>/dev/null; then
  echo "base/head commit unavailable -> full CI"
  emit true
  exit 0
fi
if ! changed="$(git diff --name-only "$base" "${HEAD_SHA}")"; then
  echo "git diff failed -> full CI"
  emit true
  exit 0
fi
echo "changed files:"
printf '%s\n' "$changed"

# Non-code (doc/harness) path denylist. A changed file that does NOT match this
# is treated as code and forces a full run. Note: Cargo.toml/Cargo.lock,
# schemas/**, .github/workflows/**, build.rs, rust-toolchain, and audit.toml are
# deliberately absent, so dependency/schema/workflow edits always run the build
# and the audit gate.
non_code='(\.md$)|(^docs/)|(^\.backlogit/)|(^\.copilot/)|(^\.github/(agents|instructions|prompts|skills|policies)/)|(^LICENSE$)'

while IFS= read -r f; do
  [ -z "$f" ] && continue
  # Anything under the source/test/bench/example trees is always treated as code
  # — even a doc-looking .md fixture — so a change to a test fixture still runs
  # the suite. The doc denylist only applies outside these trees.
  case "$f" in
    src/* | tests/* | benches/* | examples/*)
      echo "code-affecting change detected: $f"
      emit true
      exit 0
      ;;
  esac
  if ! printf '%s' "$f" | grep -Eq "$non_code"; then
    echo "code-affecting change detected: $f"
    emit true
    exit 0
  fi
done <<<"$changed"

echo "only non-code (doc/harness) paths changed"
emit false
