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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ci_detects_ci_env() {
        // Save original values
        let ci_orig = std::env::var("CI").ok();

        // Set CI environment variable
        std::env::set_var("CI", "true");
        assert!(is_ci());

        // Restore original value
        if let Some(val) = ci_orig {
            std::env::set_var("CI", val);
        } else {
            std::env::remove_var("CI");
        }
    }

    #[test]
    fn test_is_ci_detects_github_actions() {
        // Save original values
        let github_orig = std::env::var("GITHUB_ACTIONS").ok();
        let ci_orig = std::env::var("CI").ok();

        // Remove CI and set GITHUB_ACTIONS
        std::env::remove_var("CI");
        std::env::set_var("GITHUB_ACTIONS", "true");
        assert!(is_ci());

        // Restore original values
        if let Some(val) = github_orig {
            std::env::set_var("GITHUB_ACTIONS", val);
        } else {
            std::env::remove_var("GITHUB_ACTIONS");
        }
        if let Some(val) = ci_orig {
            std::env::set_var("CI", val);
        }
    }

    #[test]
    fn test_spinner_creates_progressbar() {
        let pb = spinner("Loading...");
        // Just verify it doesn't panic and returns a progress bar
        pb.finish_and_clear();
    }

    #[test]
    fn test_spinner_with_empty_message() {
        let pb = spinner("");
        pb.finish_and_clear();
    }
}
