# GraphQL Analyzer Installer for Windows
# Usage: irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "trevor-scheer/graphql-analyzer"
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\graphql-analyzer" }
$Platform = "x86_64-pc-windows-msvc"

function Get-LatestVersion {
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases"
    $cliRelease = $releases | Where-Object { $_.tag_name -like "cli/v*" } | Select-Object -First 1
    if (-not $cliRelease) {
        throw "Failed to find latest CLI release"
    }
    return $cliRelease.tag_name -replace "cli/v", ""
}

function Install-Binary {
    param(
        [string]$BinaryName,
        [string]$AssetPrefix,
        [string]$Version
    )

    $Url = "https://github.com/$Repo/releases/download/cli/v$Version/$AssetPrefix-$Platform.zip"
    $TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([System.Guid]::NewGuid().ToString()))

    try {
        Write-Host "Downloading $BinaryName..."
        $ZipPath = Join-Path $TempDir "archive.zip"
        Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

        Write-Host "Extracting $BinaryName..."
        Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        }

        $ExePath = Join-Path $TempDir "$BinaryName.exe"
        Move-Item -Path $ExePath -Destination $InstallDir -Force

        Write-Host "Installed $BinaryName to $InstallDir\$BinaryName.exe"
    }
    finally {
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Add-ToPath {
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$InstallDir*") {
        Write-Host ""
        Write-Host "Adding $InstallDir to PATH..."
        [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$InstallDir", "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "PATH updated. You may need to restart your terminal."
    }
}

Write-Host "GraphQL Analyzer Installer"
Write-Host "=========================="
Write-Host ""

$Version = Get-LatestVersion
Write-Host "Latest version: $Version"
Write-Host "Install directory: $InstallDir"
Write-Host ""

Install-Binary -BinaryName "graphql" -AssetPrefix "graphql-cli" -Version $Version
Install-Binary -BinaryName "graphql-lsp" -AssetPrefix "graphql-lsp" -Version $Version
Install-Binary -BinaryName "graphql-mcp" -AssetPrefix "graphql-mcp" -Version $Version

Add-ToPath

Write-Host ""
Write-Host "Installation complete!"
Write-Host ""
Write-Host "Run 'graphql --help' to get started."
