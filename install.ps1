# install.ps1 — graphtor-docs installer for Windows (PowerShell 5.1+)
#
# Usage:
#   irm https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.ps1 | iex
#
# Optional environment variables (set before running):
#   $env:GRAPHTOR_INSTALL_DIR  — override install directory
#   $env:GRAPHTOR_VERSION      — pin a specific release tag (default: latest stable)
#
# The installer:
#   1. Detects Windows architecture (x86_64 only for now)
#   2. Fetches the latest stable GitHub release tag (or uses GRAPHTOR_VERSION)
#   3. Downloads the .zip archive and SHA256SUMS
#   4. Verifies the SHA-256 checksum
#   5. Extracts the binary to $env:LOCALAPPDATA\graphtor-docs\bin
#   6. Adds the install directory to the user PATH (idempotent)
#
# Requires: PowerShell 5.1+ (ships with Windows 10/11) or PowerShell 7+

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Repo   = 'softwaresalt/graphtor-docs'
$Binary = 'graphtor-docs'
$Target = 'x86_64-pc-windows-msvc'

# ── Helpers ───────────────────────────────────────────────────────────────────

function Write-Info  { param([string]$Msg) Write-Host "[graphtor] $Msg" -ForegroundColor Cyan }
function Write-Warn  { param([string]$Msg) Write-Host "[graphtor] $Msg" -ForegroundColor Yellow }
function Write-Err   { param([string]$Msg) throw "[graphtor] $Msg" }

# ── Architecture check ────────────────────────────────────────────────────────

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
    Write-Err "Unsupported architecture: $arch. Only x86_64 (AMD64) Windows is supported."
}

# ── Version resolution ────────────────────────────────────────────────────────

$version = if ($env:GRAPHTOR_VERSION) { $env:GRAPHTOR_VERSION } else { $null }

if (-not $version) {
    Write-Info "Resolving latest stable release..."
    $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    $version = $release.tag_name
}

if (-not $version) {
    Write-Err "Could not determine the latest release tag. Check your network connection."
}

Write-Info "Installing $Binary $version..."

# ── Install directory ─────────────────────────────────────────────────────────

$installDir = if ($env:GRAPHTOR_INSTALL_DIR) {
    $env:GRAPHTOR_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA 'graphtor-docs\bin'
}

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    Write-Info "Created install directory: $installDir"
}

# ── Download ──────────────────────────────────────────────────────────────────

$baseUrl  = "https://github.com/$Repo/releases/download/$version"
$archive  = "$Binary-$version-$Target.zip"
$sumsFile = "SHA256SUMS"
$tmpDir   = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

try {
    $archivePath = Join-Path $tmpDir $archive
    $sumsPath    = Join-Path $tmpDir $sumsFile

    Write-Info "Downloading $archive..."
    Invoke-WebRequest "$baseUrl/$archive"   -OutFile $archivePath -UseBasicParsing
    Invoke-WebRequest "$baseUrl/$sumsFile"  -OutFile $sumsPath    -UseBasicParsing

    # ── Checksum verification ──────────────────────────────────────────────────

    Write-Info "Verifying checksum..."

    $matchedLines = @(Get-Content $sumsPath | Where-Object { $_ -like "*$archive*" })
    if ($matchedLines.Count -eq 0) {
        Write-Err "No checksum entry found for $archive in SHA256SUMS. Cannot verify download integrity."
    }
    if ($matchedLines.Count -gt 1) {
        Write-Err "Multiple checksum entries found for $archive in SHA256SUMS. Cannot verify deterministically."
    }
    $expectedLine = $matchedLines[0]
    $expected = ($expectedLine -split '\s+')[0].Trim()

    $hashResult = Get-FileHash -Path $archivePath -Algorithm SHA256
    $actual = $hashResult.Hash.ToLower()

    if ($actual -ne $expected.ToLower()) {
        Write-Err "Checksum verification FAILED.`nExpected: $expected`nActual:   $actual`nThe download may be corrupt or tampered with."
    }

    Write-Info "Checksum OK."

    # ── Extract and install ────────────────────────────────────────────────────

    Write-Info "Extracting to $installDir..."
    Expand-Archive -Path $archivePath -DestinationPath $tmpDir -Force

    $exeName = "$Binary.exe"
    $srcExe  = Join-Path $tmpDir $exeName
    $dstExe  = Join-Path $installDir $exeName

    if (-not (Test-Path $srcExe)) {
        Write-Err "Binary $exeName not found in archive. Archive contents may differ from expected."
    }

    Copy-Item -Path $srcExe -Destination $dstExe -Force

    # ── User PATH mutation (idempotent) ────────────────────────────────────────

    $userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    $pathParts = $userPath -split ';' | Where-Object { $_ -ne '' }

    if ($pathParts -notcontains $installDir) {
        $newPath = ($pathParts + $installDir) -join ';'
        [System.Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        Write-Info "Added $installDir to your user PATH."
        Write-Warn ""
        Write-Warn "PATH change will take effect in new terminal sessions."
        Write-Warn "To use $Binary now in this session, run:"
        Write-Warn "  `$env:PATH += ';$installDir'"
    } else {
        Write-Info "$installDir is already on your PATH."
    }

    Write-Info "Installed: $dstExe"
    Write-Info "Run '$Binary --help' to get started."

} finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
