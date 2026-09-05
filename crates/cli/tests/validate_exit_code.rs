//! Regression tests for `graphql validate` process exit codes.
//!
//! Originally reported in issue #1040: `graphql validate` printed
//! "Found N error(s)" but exited with status 0, breaking CI tooling that
//! relies on exit codes. These tests guard against that regression by
//! invoking the built `graphql` binary as a subprocess and asserting the
//! exit status.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_graphql");

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_validate(fixture: &str, global_args: &[&str], cmd_args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("--no-color")
        .args(global_args)
        .arg("validate")
        .args(cmd_args)
        .current_dir(fixture_dir(fixture));
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run {BIN}: {e}"))
}

fn assert_exit_code(output: &std::process::Output, expected: i32) {
    let actual = output.status.code();
    assert_eq!(
        actual,
        Some(expected),
        "expected exit code {expected}, got {actual:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn validate_exits_nonzero_on_validation_errors_human_format() {
    let output = run_validate("missing-fragment", &[], &[]);
    // ExitCode::ValidationError = 1
    assert_exit_code(&output, 1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Found 1 error(s)"),
        "expected human-format summary in stdout, got:\n{stdout}",
    );
}

#[test]
fn validate_exits_nonzero_on_validation_errors_json_format() {
    let output = run_validate("missing-fragment", &[], &["--format", "json"]);
    assert_exit_code(&output, 1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate --format json must emit valid JSON");
    assert_eq!(parsed["success"], serde_json::Value::Bool(false));
    assert_eq!(parsed["stats"]["total_errors"], serde_json::json!(1));
}

#[test]
fn validate_exits_nonzero_on_validation_errors_github_format() {
    let output = run_validate("missing-fragment", &[], &["--format", "github"]);
    assert_exit_code(&output, 1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("::error "),
        "expected GitHub Actions error annotation in stdout, got:\n{stdout}",
    );
}

#[test]
fn validate_exits_nonzero_on_validation_errors_when_quiet() {
    // --quiet suppresses the "Found N error(s)" summary; exit code must
    // still reflect that errors were found.
    let output = run_validate("missing-fragment", &["--quiet"], &[]);
    assert_exit_code(&output, 1);
}

#[test]
fn validate_exits_zero_when_no_errors() {
    let output = run_validate("valid", &[], &[]);
    assert_exit_code(&output, 0);
}
