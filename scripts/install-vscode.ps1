# GraphQL Analyzer VSCode/Cursor Extension Installer for Windows
# Usage: irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "trevor-scheer/graphql-analyzer"

# Find editor CLI (prefer cursor if both available, or use EDITOR_CLI env var)
function Find-Editor {
    if ($env:EDITOR_CLI) {
        if (Get-Command $env:EDITOR_CLI -ErrorAction SilentlyContinue) {
            return $env:EDITOR_CLI
        } else {
            throw "Specified EDITOR_CLI '$env:EDITOR_CLI' not found."
        }
    }

    if (Get-Command "cursor" -ErrorAction SilentlyContinue) {
        return "cursor"
    } elseif (Get-Command "code" -ErrorAction SilentlyContinue) {
        return "code"
    } else {
        Write-Host "Error: neither 'code' nor 'cursor' command found." -ForegroundColor Red
        Write-Host "Please install VSCode or Cursor and ensure the CLI is in your PATH."
        Write-Host "In VSCode/Cursor: Ctrl+Shift+P > 'Shell Command: Install ... command in PATH'"
        exit 1
    }
}

# Detect platform for platform-specific extension
function Get-Platform {
    # Windows only supports x64 for now
    $arch = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")

    if ($arch -eq "AMD64" -or $arch -eq "x86_64") {
        return "win32-x64"
    } else {
        throw "Unsupported Windows architecture: $arch"
    }
}

$Editor = Find-Editor
$Platform = Get-Platform

function Get-LatestVersion {
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases"
    $vscodeRelease = $releases | Where-Object { $_.tag_name -like "graphql-analyzer-vscode/v*" } | Select-Object -First 1
    if (-not $vscodeRelease) {
        throw "Failed to find latest VSCode extension release"
    }
    return $vscodeRelease.tag_name -replace "graphql-analyzer-vscode/v", ""
}

Write-Host "GraphQL Analyzer Extension Installer"
Write-Host "====================================="
Write-Host ""
Write-Host "Using: $Editor"
Write-Host "Platform: $Platform"

$Version = Get-LatestVersion
Write-Host "Latest version: $Version"
Write-Host ""

# Platform-specific extension filename: graphql-analyzer-{platform}-{version}.vsix
$VsixName = "graphql-analyzer-$Platform-$Version.vsix"
$Url = "https://github.com/$Repo/releases/download/graphql-analyzer-vscode/v$Version/$VsixName"
$TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([System.Guid]::NewGuid().ToString()))

try {
    Write-Host "Downloading extension..."
    $VsixPath = Join-Path $TempDir "graphql-analyzer.vsix"
    Invoke-WebRequest -Uri $Url -OutFile $VsixPath -UseBasicParsing

    Write-Host "Installing extension..."
    & $Editor --install-extension $VsixPath
}
finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "Done! Reload $Editor to activate the extension."
