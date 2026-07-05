#!/usr/bin/env pwsh
# Decide whether the expensive Rust CI pipeline must run for the current event.
#
# Emits `code=true|false` to $env:GITHUB_OUTPUT. Fail-safe by design: any change
# that is not a known doc/harness path forces a full run, as do scheduled/manual/
# unknown events and any case where the base ref cannot be determined. This runs
# in a lightweight gate job so a CI check is always reported (automation and
# reviewers never see a missing status) while the costly build is skipped for
# changes that touch only non-code files.
#
# Cross-platform: PowerShell (pwsh) runs both on the ubuntu CI runner and on
# Windows developer machines, so this single script is the sole source of truth.
#
# Required environment variables (provided by the workflow):
#   EVENT_NAME  github.event_name
#   BASE_SHA    github.event.pull_request.base.sha (pull_request events)
#   BEFORE_SHA  github.event.before                 (push events)
#   HEAD_SHA    github.sha
$ErrorActionPreference = 'Stop'
# Do not let git's non-zero exit codes (used deliberately below) throw.
$PSNativeCommandUseErrorActionPreference = $false

function Emit([string] $code) {
    if ($env:GITHUB_OUTPUT) {
        "code=$code" | Add-Content -Path $env:GITHUB_OUTPUT
    }
    Write-Host "run heavy build: $code"
}

switch ($env:EVENT_NAME) {
    'pull_request' { $base = $env:BASE_SHA }
    'push' { $base = $env:BEFORE_SHA }
    default {
        Write-Host "event '$($env:EVENT_NAME)' is not push/pull_request -> full CI"
        Emit 'true'
        exit 0
    }
}

$zero = '0000000000000000000000000000000000000000'
if ([string]::IsNullOrWhiteSpace($base) -or $base -eq $zero) {
    Write-Host 'no usable base ref -> full CI'
    Emit 'true'
    exit 0
}

$head = $env:HEAD_SHA

# Fail-safe: if either endpoint is unreachable (e.g. a rewritten history) or the
# diff itself errors, run the full build rather than risk skipping needed CI.
git cat-file -e "$base^{commit}" 2>$null
$baseOk = $LASTEXITCODE -eq 0
git cat-file -e "$head^{commit}" 2>$null
$headOk = $LASTEXITCODE -eq 0
if (-not ($baseOk -and $headOk)) {
    Write-Host 'base/head commit unavailable -> full CI'
    Emit 'true'
    exit 0
}

$changed = git diff --name-only $base $head
if ($LASTEXITCODE -ne 0) {
    Write-Host 'git diff failed -> full CI'
    Emit 'true'
    exit 0
}
Write-Host 'changed files:'
$changed | ForEach-Object { Write-Host $_ }

# Non-code (doc/harness) path denylist. A changed file that does NOT match this
# (and is not under a source/test tree) is treated as code and forces a full
# run. Cargo.toml/Cargo.lock, schemas/**, .github/workflows/**, build.rs,
# rust-toolchain, and audit.toml are deliberately absent, so dependency/schema/
# workflow edits always run the build and the audit gate.
$nonCode = '(\.md$)|(^docs/)|(^\.backlogit/)|(^\.copilot/)|(^\.github/(agents|instructions|prompts|skills|policies)/)|(^LICENSE$)'

foreach ($f in $changed) {
    if ([string]::IsNullOrWhiteSpace($f)) { continue }
    # Anything under the source/test/bench/example trees is always treated as
    # code — even a doc-looking .md fixture — so a change to a test fixture still
    # runs the suite. The doc denylist only applies outside these trees.
    # Case-sensitive (-cmatch/-cnotmatch) to match the original grep -E semantics
    # and avoid a case-variant path being wrongly classified as skippable doc.
    if ($f -cmatch '^(src|tests|benches|examples)/') {
        Write-Host "code-affecting change detected: $f"
        Emit 'true'
        exit 0
    }
    if ($f -cnotmatch $nonCode) {
        Write-Host "code-affecting change detected: $f"
        Emit 'true'
        exit 0
    }
}

Write-Host 'only non-code (doc/harness) paths changed'
Emit 'false'
