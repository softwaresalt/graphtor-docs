#!/usr/bin/env pwsh
# mcp-probe-ci-seeded-violation-check.ps1
#
# Windows/pwsh twin of scripts/mcp-probe-ci-seeded-violation-check.sh (the
# script actually invoked by .github/workflows/mcp-probe-ci.yml's
# probe-ci-selftest job on ubuntu-latest). Local dev convenience only; both
# scripts implement the same test-first dry-run proof for 056.028-T's
# dedicated mcp-probe CI job:
#
#   1. The job SELECTS the standalone tools/mcp-probe/Cargo.toml /
#      tools/mcp-probe/Cargo.lock manifest+lockfile (not the root workspace).
#   2. The job FAILS CLOSED on a seeded clippy::pedantic violation.
#
# Never edits tools/mcp-probe/ in place -- CI/workflow width only. All
# seeding happens in a disposable temp copy.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ProbeDir = Join-Path $RepoRoot 'tools/mcp-probe'
$WorkflowFile = Join-Path $RepoRoot '.github/workflows/mcp-probe-ci.yml'
$Toolchain = '+1.75.0'
$ClippyArgs = @('--all-targets', '--', '-D', 'warnings', '-D', 'clippy::pedantic', '-A', 'clippy::module_name_repetitions')

function Fail([string] $msg) {
    Write-Error "FAIL: $msg"
    exit 1
}

Write-Host '== Structural check: workflow selects the standalone manifest/lockfile =='
if (-not (Test-Path $WorkflowFile)) { Fail "workflow file not found: $WorkflowFile" }
$workflowText = Get-Content $WorkflowFile -Raw
if ($workflowText -notmatch [regex]::Escape('tools/mcp-probe/Cargo.toml')) {
    Fail 'workflow does not reference tools/mcp-probe/Cargo.toml'
}
if ($workflowText -notmatch [regex]::Escape('tools/mcp-probe/Cargo.lock')) {
    Fail 'workflow does not reference tools/mcp-probe/Cargo.lock'
}
Write-Host 'OK: workflow references the standalone probe manifest and lockfile.'

$ScratchDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mcp-probe-seeded-check-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $ScratchDir -Force | Out-Null
try {
    Write-Host '== Seeding a clippy::pedantic violation into a disposable scratch copy =='
    $SeedCopy = Join-Path $ScratchDir 'mcp-probe-seeded'
    New-Item -ItemType Directory -Path $SeedCopy -Force | Out-Null
    Get-ChildItem -Path $ProbeDir -Force | Where-Object { $_.Name -ne 'target' } | ForEach-Object {
        Copy-Item -Path $_.FullName -Destination $SeedCopy -Recurse -Force
    }

    $seedSnippet = @'

// Seeded by scripts/mcp-probe-ci-seeded-violation-check.ps1 (test-first red
// proof for 056.028-T). Not part of the real crate -- this file only exists
// in a disposable scratch copy and is discarded after this check runs.
// Triggers clippy::pedantic's `must_use_candidate`: a public, side-effect-free
// function whose result is not `#[must_use]`.
pub fn seeded_violation_probe_must_use_candidate(x: u32) -> u32 {
    x + 1
}
'@
    Add-Content -Path (Join-Path $SeedCopy 'src/lib.rs') -Value $seedSnippet

    Write-Host '== RED proof: seeded copy must fail the exact probe-ci clippy invocation =='
    $seededLog = Join-Path $ScratchDir 'seeded-clippy.log'
    $seedManifest = Join-Path $SeedCopy 'Cargo.toml'
    & cargo $Toolchain clippy --manifest-path $seedManifest @ClippyArgs *> $seededLog
    $seededExit = $LASTEXITCODE
    if ($seededExit -eq 0) {
        Get-Content $seededLog | Write-Host
        Fail 'seeded violation did not fail clippy -- the CI job would not have caught it'
    }
    $seededOutput = Get-Content $seededLog -Raw
    if ($seededOutput -notmatch 'must_use_candidate') {
        Write-Host $seededOutput
        Fail 'clippy failed, but not for the seeded must_use_candidate violation -- check for an unrelated regression'
    }
    Write-Host 'OK: seeded clippy::pedantic violation is caught (non-zero exit, must_use_candidate reported).'

    Write-Host '== GREEN proof: the real, unmodified crate passes the identical invocation =='
    $realManifest = Join-Path $ProbeDir 'Cargo.toml'
    & cargo $Toolchain clippy --manifest-path $realManifest @ClippyArgs
    if ($LASTEXITCODE -ne 0) {
        Fail 'the real tools/mcp-probe crate unexpectedly failed clippy -- investigate before wiring this job into required checks'
    }
    Write-Host 'OK: the real tools/mcp-probe crate passes the probe-ci clippy gate.'

    Write-Host 'PASS: mcp-probe CI seeded-violation self-test complete.'
}
finally {
    Remove-Item -Path $ScratchDir -Recurse -Force -ErrorAction SilentlyContinue
}
