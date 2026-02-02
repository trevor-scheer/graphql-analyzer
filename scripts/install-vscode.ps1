# GraphQL Analyzer VSCode Extension Installer for Windows
# Usage: irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install-vscode.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "trevor-scheer/graphql-analyzer"

# Check for code CLI
if (-not (Get-Command "code" -ErrorAction SilentlyContinue)) {
    Write-Host "Error: 'code' command not found." -ForegroundColor Red
    Write-Host "Please install VSCode and ensure 'code' is in your PATH."
    Write-Host "In VSCode: Ctrl+Shift+P > 'Shell Command: Install code command in PATH'"
    exit 1
}

function Get-LatestVersion {
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases"
    $vscodeRelease = $releases | Where-Object { $_.tag_name -like "vscode/v*" } | Select-Object -First 1
    if (-not $vscodeRelease) {
        throw "Failed to find latest VSCode extension release"
    }
    return $vscodeRelease.tag_name -replace "vscode/v", ""
}

Write-Host "GraphQL Analyzer VSCode Extension Installer"
Write-Host "============================================"
Write-Host ""

$Version = Get-LatestVersion
Write-Host "Latest version: $Version"
Write-Host ""

$Url = "https://github.com/$Repo/releases/download/vscode/v$Version/graphql-analyzer-$Version.vsix"
$TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([System.Guid]::NewGuid().ToString()))

try {
    Write-Host "Downloading extension..."
    $VsixPath = Join-Path $TempDir "graphql-analyzer.vsix"
    Invoke-WebRequest -Uri $Url -OutFile $VsixPath -UseBasicParsing

    Write-Host "Installing extension..."
    & code --install-extension $VsixPath
}
finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "Done! Reload VSCode to activate the extension."
