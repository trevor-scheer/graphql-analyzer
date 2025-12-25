use indicatif::{ProgressBar, ProgressStyle};

/// Detect if we're running in a CI environment
fn is_ci() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("CIRCLECI").is_ok()
        || std::env::var("TRAVIS").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
}

/// Create a spinner with a message
/// Returns a hidden spinner in CI environments
pub fn spinner(message: &str) -> ProgressBar {
    let pb = if is_ci() {
        ProgressBar::hidden()
    } else {
        ProgressBar::new_spinner()
    };

    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .expect("Failed to set progress style"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Create a progress bar for processing files
/// Returns a hidden progress bar in CI environments
#[allow(dead_code)]
pub fn progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = if is_ci() {
        ProgressBar::hidden()
    } else {
        ProgressBar::new(total)
    };

    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)")
            .expect("Failed to set progress style")
            .progress_chars("━━╸"),
    );
    pb.set_message(message.to_string());
    pb
}
