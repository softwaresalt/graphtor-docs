#!/usr/bin/env pwsh
# mcp-probe-ci-seeded-violation-check.ps1
#
# Windows/pwsh twin of scripts/mcp-probe-ci-seeded-violation-check.sh (the
# script actually invoked by .github/workflows/mcp-probe-ci.yml's
# probe-ci-selftest job on ubuntu-latest). Local dev convenience only; both
# scripts implement the same test-first dry-run proof for 056.028-T's
# dedicated mcp-probe CI job:
#
#   1. The probe-ci job's clippy/test/build/audit steps are actually bound
#      to the standalone tools/mcp-probe/Cargo.toml / Cargo.lock
#      manifest+lockfile (not the root workspace), with --locked and the
#      required lint/audit flags present in each specific step's own
#      command block.
#   2. The job FAILS CLOSED on a seeded clippy::pedantic violation.
#   3. The disposable scratch copy used to seed that violation is never
#      built from a probe tree containing a symlink/reparse point.
#
# Never edits tools/mcp-probe/ in place -- CI/workflow width only. All
# seeding happens in a disposable temp copy.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ProbeDir = Join-Path $RepoRoot 'tools/mcp-probe'
$WorkflowFile = Join-Path $RepoRoot '.github/workflows/mcp-probe-ci.yml'
$Toolchain = '+1.75.0'
$ClippyArgs = @('--locked', '--all-targets', '--', '-D', 'warnings', '-D', 'clippy::pedantic', '-A', 'clippy::module_name_repetitions')

function Fail([string] $msg) {
    Write-Error "FAIL: $msg"
    exit 1
}

# Returns the lines belonging to the named workflow step (from its
# `- name: <step>` marker, exclusive, up to but excluding the next
# `- name:` line or a job-level key line), so structural assertions below
# are bound to that exact step's command instead of matching text anywhere
# in the file. Step names used here (clippy/test/build/audit) are unique
# to the probe-ci job.
function Get-StepBlock([string[]] $Lines, [string] $StepName) {
    $marker = "- name: $StepName"
    $capturing = $false
    $block = New-Object System.Collections.Generic.List[string]
    foreach ($line in $Lines) {
        if (-not $capturing) {
            if ($line.TrimStart() -eq $marker) { $capturing = $true }
            continue
        }
        if ($line -match '^\s*- name:') { break }
        if ($line -match '^  [A-Za-z0-9_-]+:') { break }
        $block.Add($line)
    }
    return ($block -join "`n")
}

function Assert-StepContains([string[]] $Lines, [string] $StepName, [string] $Needle) {
    $block = Get-StepBlock -Lines $Lines -StepName $StepName
    if ([string]::IsNullOrEmpty($block)) { Fail "workflow step '$StepName' not found" }
    if (-not $block.Contains($Needle)) {
        Fail "workflow step '$StepName' does not contain expected: $Needle"
    }
}

# Returns reparse points (symlinks/junctions) under $RootPath, excluding the
# exact top-level $RootPath/target directory (build output, not source) --
# mirrors the Bash twin's `find "$PROBE_DIR" -path "$PROBE_DIR/target"
# -prune -o -type l -print`.
function Get-SymlinksExcludingTarget([string] $RootPath) {
    $targetPath = Join-Path $RootPath 'target'
    $result = New-Object System.Collections.Generic.List[string]
    $stack = New-Object System.Collections.Generic.Stack[string]
    $stack.Push($RootPath)
    while ($stack.Count -gt 0) {
        $current = $stack.Pop()
        $items = Get-ChildItem -Path $current -Force -ErrorAction SilentlyContinue
        foreach ($item in $items) {
            if ($item.FullName -eq $targetPath) { continue }
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                $result.Add($item.FullName)
                continue
            }
            if ($item.PSIsContainer) {
                $stack.Push($item.FullName)
            }
        }
    }
    return $result
}

Write-Host '== Structural check: probe-ci steps bind the standalone manifest/lockfile, --locked, and lint/audit flags =='
if (-not (Test-Path $WorkflowFile)) { Fail "workflow file not found: $WorkflowFile" }
$workflowLines = Get-Content $WorkflowFile
Assert-StepContains -Lines $workflowLines -StepName 'clippy' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-StepContains -Lines $workflowLines -StepName 'clippy' -Needle '--locked'
Assert-StepContains -Lines $workflowLines -StepName 'clippy' -Needle '-D warnings'
Assert-StepContains -Lines $workflowLines -StepName 'clippy' -Needle '-D clippy::pedantic'
Assert-StepContains -Lines $workflowLines -StepName 'test' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-StepContains -Lines $workflowLines -StepName 'test' -Needle '--locked'
Assert-StepContains -Lines $workflowLines -StepName 'build' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-StepContains -Lines $workflowLines -StepName 'build' -Needle '--locked'
Assert-StepContains -Lines $workflowLines -StepName 'audit' -Needle '--file tools/mcp-probe/Cargo.lock'
Assert-StepContains -Lines $workflowLines -StepName 'audit' -Needle '--deny warnings'
Write-Host 'OK: probe-ci steps are bound to the standalone manifest/lockfile, --locked, and the required lint/audit flags.'

$ScratchDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mcp-probe-seeded-check-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $ScratchDir -Force | Out-Null
try {
    Write-Host '== Symlink safety check: refuse to seed if the probe tree contains a symlink/reparse point =='
    # Copy-Item -Recurse below can follow reparse points; a symlink at e.g.
    # src/lib.rs pointing outside the crate would make the later
    # Add-Content append write through it -- a path-traversal risk if such a
    # symlink were ever introduced. Fail closed rather than dereferencing or
    # otherwise trying to sanitize it. Excludes the top-level target/ dir to
    # match the copy loop's own exclusion below (build output, not source).
    $symlinks = Get-SymlinksExcludingTarget -RootPath $ProbeDir
    if ($symlinks.Count -gt 0) {
        $symlinks | ForEach-Object { Write-Host $_ }
        Fail 'tools/mcp-probe contains one or more symlinks/reparse points -- refusing to seed a scratch copy (potential path traversal via copy/append)'
    }
    Write-Host 'OK: no symlinks/reparse points found under tools/mcp-probe (excluding target/).'

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
