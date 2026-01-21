use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Development tasks for graphql-lsp")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the LSP server and install the `VSCode` extension
    Install {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },
    /// Build release artifacts locally (cargo-dist + `VSCode` extension)
    Release {
        /// Specific target triple to build for (defaults to host target)
        #[arg(long, conflicts_with = "all_targets")]
        target: Option<String>,
        /// Build for all targets defined in dist-workspace.toml (requires cross-compilation toolchains)
        #[arg(long, conflicts_with = "target")]
        all_targets: bool,
        /// Skip cargo-dist build (only package `VSCode` extension)
        #[arg(long)]
        skip_dist: bool,
        /// Skip `VSCode` extension packaging
        #[arg(long)]
        skip_vscode: bool,
        /// Create a GitHub release and upload artifacts (requires gh CLI)
        #[arg(long)]
        publish: bool,
        /// Git tag for the release (defaults to version from Cargo.toml, e.g., v0.1.0-alpha.0)
        #[arg(long)]
        tag: Option<String>,
    },
}

#[allow(clippy::struct_excessive_bools)]
struct ReleaseOptions {
    target: Option<String>,
    all_targets: bool,
    skip_dist: bool,
    skip_vscode: bool,
    publish: bool,
    tag: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { release } => install(release),
        Commands::Release {
            target,
            all_targets,
            skip_dist,
            skip_vscode,
            publish,
            tag,
        } => release(ReleaseOptions {
            target,
            all_targets,
            skip_dist,
            skip_vscode,
            publish,
            tag,
        }),
    }
}

fn project_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .expect("xtask should be in project root")
        .to_path_buf()
}

fn install(release: bool) -> Result<()> {
    let root = project_root();
    let vscode_dir = root.join("editors/vscode");

    // Step 1: Build cargo
    println!("Building LSP server...");
    let mut cargo_args = vec!["build", "--package", "graphql-lsp"];
    if release {
        cargo_args.push("--release");
    }

    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(&root)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        bail!("cargo build failed");
    }

    // Step 2: Compile TypeScript
    println!("Compiling VSCode extension...");
    let status = Command::new("npm")
        .args(["run", "compile"])
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run npm compile")?;

    if !status.success() {
        bail!("npm run compile failed");
    }

    // Step 3: Package extension
    println!("Packaging extension...");
    let status = Command::new("npm")
        .args(["run", "package"])
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run npm package")?;

    if !status.success() {
        bail!("npm run package failed");
    }

    // Step 4: Find the .vsix file
    let vsix_file = find_vsix(&vscode_dir)?;
    println!("Found package: {}", vsix_file.display());

    // Step 5: Install the extension
    println!("Installing extension...");
    let status = Command::new("code")
        .args(["--install-extension", &vsix_file.to_string_lossy()])
        .status()
        .context("Failed to run code --install-extension")?;

    if !status.success() {
        bail!("Extension installation failed");
    }

    println!("Extension installed successfully!");
    println!("Restart VSCode or reload the window to use the new version.");

    Ok(())
}

fn find_vsix(vscode_dir: &Path) -> Result<PathBuf> {
    let entries = std::fs::read_dir(vscode_dir).context("Failed to read vscode directory")?;

    let mut vsix_files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vsix"))
        .collect();

    if vsix_files.is_empty() {
        bail!("No .vsix file found in {}", vscode_dir.display());
    }

    // Sort by modification time, newest first
    vsix_files.sort_by(|a, b| {
        let a_time = a.metadata().and_then(|m| m.modified()).ok();
        let b_time = b.metadata().and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    Ok(vsix_files.remove(0))
}

fn release(opts: ReleaseOptions) -> Result<()> {
    let root = project_root();
    let output_dir = root.join("target/release-artifacts");

    // Get version from Cargo.toml
    let version = get_workspace_version(&root)?;
    let release_tag = opts.tag.unwrap_or_else(|| format!("v{version}"));

    println!("Version: {version}");
    println!("Release tag: {release_tag}");

    // Create output directory
    std::fs::create_dir_all(&output_dir).context("Failed to create output directory")?;
    println!(
        "Release artifacts will be collected in: {}",
        output_dir.display()
    );

    let mut artifacts: Vec<PathBuf> = Vec::new();

    // Step 1: Build with cargo-dist
    if !opts.skip_dist {
        println!("\n=== Building with cargo-dist ===");
        let targets = if opts.all_targets {
            get_dist_targets(&root)?
        } else if let Some(t) = opts.target {
            vec![t]
        } else {
            vec![] // Empty means host target
        };
        artifacts.extend(build_cargo_dist(&root, &output_dir, &targets)?);
    }

    // Step 2: Package VSCode extension
    if !opts.skip_vscode {
        println!("\n=== Packaging VSCode extension ===");
        artifacts.push(package_vscode_extension(&root, &output_dir)?);
    }

    // Print summary
    println!("\n=== Release artifacts ===");
    for artifact in &artifacts {
        let size =
            std::fs::metadata(artifact).map_or_else(|_| "?".to_string(), |m| format_size(m.len()));
        println!("  {} ({size})", artifact.display());
    }

    // Step 3: Publish to GitHub if requested
    if opts.publish {
        println!("\n=== Publishing to GitHub ===");
        publish_to_github(&root, &release_tag, &artifacts)?;
    }

    println!("\nRelease build complete!");
    Ok(())
}

fn get_workspace_version(root: &Path) -> Result<String> {
    let cargo_toml_path = root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path).context("Failed to read Cargo.toml")?;
    let parsed: Value = content.parse().context("Failed to parse Cargo.toml")?;

    parsed
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .context("Could not find workspace.package.version in Cargo.toml")
}

fn get_dist_targets(root: &Path) -> Result<Vec<String>> {
    let dist_toml_path = root.join("dist-workspace.toml");
    let content =
        std::fs::read_to_string(&dist_toml_path).context("Failed to read dist-workspace.toml")?;
    let parsed: Value = content
        .parse()
        .context("Failed to parse dist-workspace.toml")?;

    let targets = parsed
        .get("dist")
        .and_then(|d| d.get("targets"))
        .and_then(|t| t.as_array())
        .context("Could not find dist.targets in dist-workspace.toml")?;

    let target_strings: Vec<String> = targets
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    if target_strings.is_empty() {
        bail!("No targets found in dist-workspace.toml");
    }

    Ok(target_strings)
}

fn build_cargo_dist(root: &Path, output_dir: &Path, targets: &[String]) -> Result<Vec<PathBuf>> {
    // Check if cargo-dist is installed
    let check = Command::new("cargo").args(["dist", "--version"]).output();

    if check.is_err() || !check.unwrap().status.success() {
        println!("cargo-dist not found. Installing...");
        let status = Command::new("cargo")
            .args(["install", "cargo-dist"])
            .status()
            .context("Failed to install cargo-dist")?;
        if !status.success() {
            bail!("Failed to install cargo-dist");
        }
    }

    // Build with cargo-dist
    let mut args = vec![
        "dist".to_string(),
        "build".to_string(),
        "--output-format=json".to_string(),
    ];

    if targets.is_empty() {
        println!("Building for host target...");
    } else {
        println!("Building for targets: {}", targets.join(", "));
        if targets.len() > 1 {
            println!("Note: Cross-compilation requires appropriate toolchains to be installed.");
            println!("      Consider using 'cross' or platform-specific toolchains.");
        }
        for t in targets {
            args.push("--target".to_string());
            args.push(t.clone());
        }
    }

    println!("Running: cargo {}", args.join(" "));
    let output = Command::new("cargo")
        .args(&args)
        .current_dir(root)
        .output()
        .context("Failed to run cargo dist build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("cargo dist build failed:\n{stderr}");
    }

    // Parse JSON output to find artifacts
    let stdout = String::from_utf8_lossy(&output.stdout);
    let dist_dir = root.join("target/distrib");

    let mut collected = Vec::new();

    // Copy all artifacts from target/distrib to our output directory
    if dist_dir.exists() {
        for entry in std::fs::read_dir(&dist_dir).context("Failed to read distrib directory")? {
            let entry = entry?;
            let path = entry.path();

            // Skip directories, only copy files
            if path.is_file() {
                let filename = path.file_name().unwrap();
                let dest = output_dir.join(filename);
                std::fs::copy(&path, &dest)
                    .with_context(|| format!("Failed to copy {}", path.display()))?;
                println!("  Copied: {}", filename.to_string_lossy());
                collected.push(dest);
            }
        }
    }

    // Also check for platform-specific subdirectories
    for entry in std::fs::read_dir(&dist_dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            for subentry in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                let subpath = subentry.path();
                if subpath.is_file() {
                    let filename = subpath.file_name().unwrap();
                    let dest = output_dir.join(filename);
                    if !dest.exists() {
                        std::fs::copy(&subpath, &dest)
                            .with_context(|| format!("Failed to copy {}", subpath.display()))?;
                        println!("  Copied: {}", filename.to_string_lossy());
                        collected.push(dest);
                    }
                }
            }
        }
    }

    if collected.is_empty() {
        println!("  Warning: No artifacts found in {}", dist_dir.display());
        println!("  cargo-dist output:\n{stdout}");
    }

    Ok(collected)
}

fn package_vscode_extension(root: &Path, output_dir: &Path) -> Result<PathBuf> {
    let vscode_dir = root.join("editors/vscode");

    // Install npm dependencies if needed
    let node_modules = vscode_dir.join("node_modules");
    if !node_modules.exists() {
        println!("Installing npm dependencies...");
        let status = Command::new("npm")
            .args(["install"])
            .current_dir(&vscode_dir)
            .status()
            .context("Failed to run npm install")?;
        if !status.success() {
            bail!("npm install failed");
        }
    }

    // Compile TypeScript
    println!("Compiling TypeScript...");
    let status = Command::new("npm")
        .args(["run", "compile"])
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run npm compile")?;

    if !status.success() {
        bail!("npm run compile failed");
    }

    // Package extension
    println!("Packaging extension...");
    let status = Command::new("npm")
        .args(["run", "package"])
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run npm package")?;

    if !status.success() {
        bail!("npm run package failed");
    }

    // Find and copy the .vsix file
    let vsix_file = find_vsix(&vscode_dir)?;
    let filename = vsix_file.file_name().unwrap();
    let dest = output_dir.join(filename);
    std::fs::copy(&vsix_file, &dest)
        .with_context(|| format!("Failed to copy {}", vsix_file.display()))?;
    println!("  Copied: {}", filename.to_string_lossy());

    Ok(dest)
}

fn publish_to_github(root: &Path, tag: &str, artifacts: &[PathBuf]) -> Result<()> {
    // Check if gh CLI is available
    let check = Command::new("gh").arg("--version").output();
    if check.is_err() || !check.unwrap().status.success() {
        bail!("GitHub CLI (gh) not found. Install it from https://cli.github.com/");
    }

    // Check if tag exists
    let tag_check = Command::new("git")
        .args(["tag", "-l", tag])
        .current_dir(root)
        .output()
        .context("Failed to check git tag")?;

    let tag_exists = !String::from_utf8_lossy(&tag_check.stdout).trim().is_empty();

    if !tag_exists {
        println!("Creating tag {tag}...");
        let status = Command::new("git")
            .args(["tag", tag])
            .current_dir(root)
            .status()
            .context("Failed to create git tag")?;
        if !status.success() {
            bail!("Failed to create tag {tag}");
        }

        println!("Pushing tag to origin...");
        let status = Command::new("git")
            .args(["push", "origin", tag])
            .current_dir(root)
            .status()
            .context("Failed to push git tag")?;
        if !status.success() {
            bail!("Failed to push tag {tag}");
        }
    }

    // Create GitHub release
    println!("Creating GitHub release {tag}...");
    let status = Command::new("gh")
        .args([
            "release",
            "create",
            tag,
            "--title",
            tag,
            "--generate-notes",
            "--repo",
            "trevor-scheer/graphql-lsp",
        ])
        .current_dir(root)
        .status()
        .context("Failed to create GitHub release")?;

    if !status.success() {
        // Release might already exist, try to upload anyway
        println!("Release may already exist, attempting to upload artifacts...");
    }

    // Upload artifacts
    println!("Uploading artifacts...");
    let artifact_paths: Vec<&str> = artifacts.iter().map(|p| p.to_str().unwrap()).collect();
    let mut args = vec![
        "release",
        "upload",
        tag,
        "--clobber",
        "--repo",
        "trevor-scheer/graphql-lsp",
    ];
    args.extend(artifact_paths);

    let status = Command::new("gh")
        .args(&args)
        .current_dir(root)
        .status()
        .context("Failed to upload artifacts")?;

    if !status.success() {
        bail!("Failed to upload artifacts to release");
    }

    println!("Published release: https://github.com/trevor-scheer/graphql-lsp/releases/tag/{tag}");
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
