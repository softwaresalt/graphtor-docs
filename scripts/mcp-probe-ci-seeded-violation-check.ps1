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
#      *executable* command text -- extracted from the `run:` value only,
#      with comment lines stripped, so a stale descriptive comment
#      mentioning the same flags can never satisfy the check on its own.
#   2. The job FAILS CLOSED on a seeded clippy::pedantic violation, using
#      the clippy step's *actual* extracted `run:` command (manifest path
#      substituted to point at the scratch copy) rather than a separately
#      hand-maintained mirror of its flags.
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
$RealManifestRel = 'tools/mcp-probe/Cargo.toml'

function Fail([string] $msg) {
    Write-Error "FAIL: $msg"
    exit 1
}

# Returns only the *executable* text of the named workflow step's `run:`
# value (single-line `run: <cmd>` or multi-line `run: |` block scalar), as
# an array of lines with comment lines (leading `#`, after left-trim)
# filtered out and each line's shared block-scalar indentation stripped.
# Never returns text from outside the `run:` key itself -- in particular,
# never the step's own preceding descriptive `#` comments, which live above
# `run:` at the same indentation and are excluded by construction, not
# merely by a post-hoc comment filter. Step names used here
# (clippy/test/build/audit) are unique to the probe-ci job.
function Get-RunBody([string[]] $Lines, [string] $StepName) {
    $marker = "- name: $StepName"
    $capturing = $false
    $inRun = $false
    $runIndent = -1
    $body = New-Object System.Collections.Generic.List[string]
    foreach ($line in $Lines) {
        if (-not $capturing -and -not $inRun) {
            if ($line.TrimStart() -eq $marker) { $capturing = $true }
            continue
        }
        if ($capturing -and -not $inRun) {
            if ($line -match '^\s*- name:') { break }
            if ($line -match '^  [A-Za-z0-9_-]+:') { break }
            if ($line -match '^\s*run:\s*\|\s*$') {
                $inRun = $true
                $runIndent = ($line -replace 'run:.*$', '').Length
                continue
            }
            if ($line -match '^\s*run:\s*(.*)$') {
                $val = $Matches[1]
                if ($val -notmatch '^\s*#') { $body.Add($val) }
                break
            }
            continue
        }
        if ($inRun) {
            if ($line -match '^\s*$') { continue }
            $curIndent = ($line -replace '[^ ].*$', '').Length
            if ($curIndent -le $runIndent) { break }
            $trimmed = $line.TrimStart()
            if ($trimmed -notmatch '^#') { $body.Add($trimmed) }
        }
    }
    return ($body -join "`n")
}

function Assert-RunBodyContains([string[]] $Lines, [string] $StepName, [string] $Needle) {
    $body = Get-RunBody -Lines $Lines -StepName $StepName
    if ([string]::IsNullOrEmpty($body)) { Fail "workflow step '$StepName' run: value not found" }
    if (-not $body.Contains($Needle)) {
        Fail "workflow step '$StepName' run: command does not contain expected: $Needle"
    }
}

# Joins a (possibly multi-line, backslash-continued) run: body into one
# single-line shell-style command string, for direct invocation.
function ConvertTo-SingleLineCommand([string] $RunBody) {
    $lines = $RunBody -split "`n" | ForEach-Object { $_ -replace '\\\s*$', '' }
    return (($lines -join ' ') -replace '\s+', ' ').Trim()
}

# Returns reparse points (symlinks/junctions) under $RootPath, excluding the
# exact top-level $RootPath/target directory (build output, not source) --
# mirrors the Bash twin's `find "$PROBE_DIR" -path "$PROBE_DIR/target"
# -prune -o -type l -print`.
function Get-SymlinksExcludingTarget([string] $RootPath) {
    $targetPath = Join-Path $RootPath 'target'
    $result = New-Object System.Collections.Generic.List[string]
    # Check the root itself first -- a junction/symlink AT $RootPath (e.g. a
    # crate-root reparse point) is never visited by Get-ChildItem against its
    # own parent below, so without this explicit check it would be silently
    # traversed and copied, defeating the fail-closed guard entirely.
    $rootItem = Get-Item -Path $RootPath -Force -ErrorAction SilentlyContinue
    if ($null -ne $rootItem -and ($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        $result.Add($rootItem.FullName)
        return $result
    }
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

Write-Host '== Structural check: probe-ci steps'' actual run: commands bind the standalone manifest/lockfile, --locked, and lint/audit flags =='
if (-not (Test-Path $WorkflowFile)) { Fail "workflow file not found: $WorkflowFile" }
$workflowLines = Get-Content $WorkflowFile
Assert-RunBodyContains -Lines $workflowLines -StepName 'clippy' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-RunBodyContains -Lines $workflowLines -StepName 'clippy' -Needle '--locked'
Assert-RunBodyContains -Lines $workflowLines -StepName 'clippy' -Needle '-D warnings'
Assert-RunBodyContains -Lines $workflowLines -StepName 'clippy' -Needle '-D clippy::pedantic'
Assert-RunBodyContains -Lines $workflowLines -StepName 'test' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-RunBodyContains -Lines $workflowLines -StepName 'test' -Needle '--locked'
Assert-RunBodyContains -Lines $workflowLines -StepName 'build' -Needle '--manifest-path tools/mcp-probe/Cargo.toml'
Assert-RunBodyContains -Lines $workflowLines -StepName 'build' -Needle '--locked'
Assert-RunBodyContains -Lines $workflowLines -StepName 'audit' -Needle '--file tools/mcp-probe/Cargo.lock'
Assert-RunBodyContains -Lines $workflowLines -StepName 'audit' -Needle '--deny warnings'
Write-Host 'OK: probe-ci run: commands are bound to the standalone manifest/lockfile, --locked, and the required lint/audit flags (comments excluded from the check).'

$ClippyRunBody = Get-RunBody -Lines $workflowLines -StepName 'clippy'
if ([string]::IsNullOrEmpty($ClippyRunBody)) { Fail 'could not extract the clippy step''s run: command from the workflow' }
$ClippyCommandLine = ConvertTo-SingleLineCommand $ClippyRunBody

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

    Write-Host '== RED proof: the workflow''s own extracted clippy command, pointed at the seeded copy, must fail =='
    # Substitutes only the manifest path so the executed command is
    # otherwise byte-identical to what probe-ci actually runs -- this is
    # "share one executable command with the workflow" rather than a
    # separately maintained mirror of its flags. The substituted value is
    # kept as the bare relative "Cargo.toml" (executed with cwd=$SeedCopy)
    # rather than an absolute $env:TEMP-rooted path: %TEMP%/user-profile
    # paths can legitimately contain spaces on Windows, and the naive
    # whitespace tokenizer below would otherwise split such a path into
    # multiple arguments and fail the red proof before clippy even runs --
    # indistinguishable from a genuinely missing seeded lint.
    $seededCommandLine = $ClippyCommandLine.Replace($RealManifestRel, 'Cargo.toml')
    $seededTokens = $seededCommandLine -split '\s+'
    $seededLog = Join-Path $ScratchDir 'seeded-clippy.log'
    Push-Location $SeedCopy
    try {
        & $seededTokens[0] @($seededTokens[1..($seededTokens.Length - 1)]) *> $seededLog
        $seededExit = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    if ($seededExit -eq 0) {
        Get-Content $seededLog | Write-Host
        Fail 'seeded violation did not fail clippy -- the CI job would not have caught it'
    }
    $seededOutput = Get-Content $seededLog -Raw
    if ($seededOutput -notmatch 'must_use_candidate') {
        Write-Host $seededOutput
        Fail 'clippy failed, but not for the seeded must_use_candidate violation -- check for an unrelated regression'
    }
    Write-Host 'OK: seeded clippy::pedantic violation is caught (non-zero exit, must_use_candidate reported) by the workflow''s own extracted command.'

    Write-Host '== GREEN proof: the workflow''s own extracted clippy command passes against the real, unmodified crate =='
    Push-Location $RepoRoot
    try {
        $realTokens = $ClippyCommandLine -split '\s+'
        & $realTokens[0] @($realTokens[1..($realTokens.Length - 1)])
        if ($LASTEXITCODE -ne 0) {
            Fail 'the real tools/mcp-probe crate unexpectedly failed clippy -- investigate before wiring this job into required checks'
        }
    }
    finally {
        Pop-Location
    }
    Write-Host 'OK: the real tools/mcp-probe crate passes the probe-ci clippy gate (using the workflow''s own extracted command, executed unmodified).'

    Write-Host 'PASS: mcp-probe CI seeded-violation self-test complete.'
}
finally {
    Remove-Item -Path $ScratchDir -Recurse -Force -ErrorAction SilentlyContinue
}
