# GraphQL Analyzer Binary Installer for Windows
#
# Install the latest CLI:
#   irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.ps1 | iex
#
# Install a specific tool (set env var before piping):
#   $env:GA_TOOL="lsp"; irm .../install.ps1 | iex
#
# Install a specific version:
#   $env:GA_TOOL="cli"; $env:GA_VERSION="0.1.6"; irm .../install.ps1 | iex
#
# Environment variables:
#   GA_TOOL     - Tool to install: cli (default), lsp, mcp
#   GA_VERSION  - Version to install (default: latest)
#   INSTALL_DIR - Override install directory

$ErrorActionPreference = "Stop"

$Repo = "trevor-scheer/graphql-analyzer"
$Tool = if ($env:GA_TOOL) { $env:GA_TOOL } else { "cli" }
$Version = $env:GA_VERSION
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\graphql-analyzer" }
$Platform = "x86_64-pc-windows-msvc"

# Map tool name to release tag prefix, artifact prefix, and binary name
switch ($Tool) {
    "cli" {
        $TagPrefix = "graphql-analyzer-cli"
        $ArtifactPrefix = "graphql-cli"
        $BinaryName = "graphql"
    }
    "lsp" {
        $TagPrefix = "graphql-analyzer-lsp"
        $ArtifactPrefix = "graphql-lsp"
        $BinaryName = "graphql-lsp"
    }
    "mcp" {
        $TagPrefix = "graphql-analyzer-mcp"
        $ArtifactPrefix = "graphql-mcp"
        $BinaryName = "graphql-mcp"
    }
    default {
        Write-Host "Unknown tool: $Tool" -ForegroundColor Red
        Write-Host "Valid tools: cli, lsp, mcp"
        exit 1
    }
}

function Get-LatestVersion {
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases"
    $release = $releases | Where-Object { $_.tag_name -like "$TagPrefix/v*" } | Select-Object -First 1
    if (-not $release) {
        throw "Failed to find latest $Tool release"
    }
    return $release.tag_name -replace "$TagPrefix/v", ""
}

Write-Host "GraphQL Analyzer Installer"
Write-Host "=========================="
Write-Host ""
Write-Host "Tool:     $Tool ($BinaryName)"
Write-Host "Platform: $Platform"

if (-not $Version) {
    $Version = Get-LatestVersion
}
Write-Host "Version:  $Version"
Write-Host ""

$Url = "https://github.com/$Repo/releases/download/$TagPrefix/v$Version/$ArtifactPrefix-$Platform.zip"
$TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([System.Guid]::NewGuid().ToString()))

try {
    Write-Host "Downloading $BinaryName..."
    $ZipPath = Join-Path $TempDir "archive.zip"
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    Move-Item -Path (Join-Path $TempDir "$BinaryName.exe") -Destination $InstallDir -Force
    Write-Host "Installed $BinaryName to $InstallDir\$BinaryName.exe"
}
catch {
    Write-Host "Failed to download from $Url" -ForegroundColor Red
    Write-Host ""
    Write-Host "Check that version $Version exists at:"
    Write-Host "  https://github.com/$Repo/releases/tag/$TagPrefix/v$Version"
    throw
}
finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

# Add to PATH if not already there
$CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($CurrentPath -notlike "*$InstallDir*") {
    Write-Host ""
    Write-Host "Adding $InstallDir to PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "PATH updated. You may need to restart your terminal."
}

Write-Host ""
Write-Host "Run '$BinaryName --help' to get started."

# Clean up env vars so they don't leak into subsequent invocations
$env:GA_TOOL = $null
$env:GA_VERSION = $null
