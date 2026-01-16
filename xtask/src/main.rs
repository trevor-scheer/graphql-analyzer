use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { release } => install(release),
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

fn find_vsix(vscode_dir: &PathBuf) -> Result<PathBuf> {
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
