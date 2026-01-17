# Watch Mode Exploration

**Issue**: #423
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores adding a watch mode that runs as a persistent process, watching files for changes and providing instant feedback.

## Goals

1. Continuous validation during development
2. Faster feedback than repeated CLI invocations
3. Desktop notifications on errors
4. Foundation for editor-agnostic tooling

## Technical Analysis

### Architecture

```
┌─────────────────────────────────┐
│  Watch Command                  │
│  - File watcher (notify)        │
│  - Debouncing                   │
│  - Output formatting            │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  Persistent AnalysisHost        │
│  - Salsa database (cached)      │
│  - Incremental updates          │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  graphql-ide                    │
│  - diagnostics()                │
│  - File change handling         │
└─────────────────────────────────┘
```

### Performance Benefits

With a persistent Salsa database:

| Scenario | Cold (CLI) | Warm (Watch) | Speedup |
|----------|------------|--------------|---------|
| First validation | 100ms | 100ms | 1x |
| Single file change | 100ms | 5ms | 20x |
| Schema-only change | 100ms | 50ms | 2x |
| Dependent file change | 100ms | 10ms | 10x |

The watch mode maintains cache between validations, so only affected queries recompute.

## CLI Interface

### Basic Usage

```bash
# Watch with default settings
graphql watch

# Watch specific project (multi-project config)
graphql watch --project frontend

# Custom config file
graphql watch --config .graphqlrc.yaml
```

### Output Formats

```bash
# Pretty output (default, colored)
graphql watch --format=pretty

# JSON for programmatic consumption
graphql watch --format=json

# JSON Lines (streaming)
graphql watch --format=jsonl

# Minimal (errors only)
graphql watch --format=minimal
```

### Filtering

```bash
# Only show errors, not warnings
graphql watch --only-errors

# Watch specific paths
graphql watch --include "src/**/*.graphql"

# Exclude paths
graphql watch --exclude "**/*.test.graphql"

# Combine filters
graphql watch --include "src/**" --exclude "**/generated/**"
```

### Notifications

```bash
# Desktop notifications on errors
graphql watch --notify

# Sound alert (macOS/Linux with paplay/afplay)
graphql watch --notify --sound
```

## Output Formats

### Pretty Format (Default)

```
┌─────────────────────────────────────────────────────────────┐
│ GraphQL Watch Mode                                          │
│ Watching: src/**/*.graphql, src/**/*.{ts,tsx}              │
└─────────────────────────────────────────────────────────────┘

[12:34:56] Watching for changes... (45 files)

[12:34:58] src/queries/user.graphql changed
           ✓ Valid

[12:35:02] src/queries/post.graphql changed
           ✗ 2 errors

  error[E001]: Unknown field "nonExistent" on type "Post"
    --> src/queries/post.graphql:2:5
     |
   2 |     nonExistent
     |     ^^^^^^^^^^^ field does not exist
     |
     = help: did you mean "title" or "content"?

  error[E002]: Fragment "UserFields" is not defined
    --> src/queries/post.graphql:5:10
     |
   5 |     ...UserFields
     |        ^^^^^^^^^^ unknown fragment
     |

[12:35:15] src/queries/post.graphql changed
           ✓ Valid (fixed 2 errors)

[12:35:30] src/fragments/user.graphql changed
           Validating 3 dependent files...
           ✓ All valid
```

### JSON Lines Format

```jsonl
{"type":"start","timestamp":"2024-01-17T12:34:56Z","files":45}
{"type":"change","timestamp":"2024-01-17T12:34:58Z","file":"src/queries/user.graphql","valid":true,"errors":[],"warnings":[]}
{"type":"change","timestamp":"2024-01-17T12:35:02Z","file":"src/queries/post.graphql","valid":false,"errors":[{"severity":"error","code":"E001","message":"Unknown field \"nonExistent\"","file":"src/queries/post.graphql","line":2,"column":5}]}
{"type":"change","timestamp":"2024-01-17T12:35:15Z","file":"src/queries/post.graphql","valid":true,"errors":[],"warnings":[],"fixed":2}
```

### Minimal Format

```
[12:35:02] ✗ src/queries/post.graphql:2:5 - Unknown field "nonExistent"
[12:35:02] ✗ src/queries/post.graphql:5:10 - Fragment "UserFields" is not defined
[12:35:15] ✓ src/queries/post.graphql
```

## Implementation

### Dependencies

```toml
# Cargo.toml additions
[dependencies]
notify = "6"           # File system notifications
notify-debouncer-mini = "0.4"  # Debouncing
notify-rust = "4"      # Desktop notifications (optional)
crossterm = "0.27"     # Terminal colors/formatting
```

### Core Watch Loop

```rust
// crates/graphql-cli/src/commands/watch.rs

use notify::{Watcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
use std::time::Duration;

pub fn run_watch(config: WatchConfig) -> Result<()> {
    // Initialize persistent analysis
    let mut host = AnalysisHost::new();
    load_initial_files(&mut host, &config)?;

    // Initial validation
    let diagnostics = validate_all(&host);
    print_diagnostics(&diagnostics, &config.format);

    // Set up file watcher
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(
        Duration::from_millis(100),
        move |events| tx.send(events).unwrap()
    )?;

    debouncer.watcher().watch(
        Path::new(&config.root),
        RecursiveMode::Recursive
    )?;

    println!("[{}] Watching for changes...", now());

    // Watch loop
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    handle_file_event(&mut host, event, &config)?;
                }
            }
            Ok(Err(e)) => eprintln!("Watch error: {}", e),
            Err(_) => break, // Channel closed
        }
    }

    Ok(())
}

fn handle_file_event(
    host: &mut AnalysisHost,
    event: DebouncedEvent,
    config: &WatchConfig,
) -> Result<()> {
    let path = &event.path;

    // Skip non-GraphQL files
    if !is_graphql_file(path) && !is_source_file(path) {
        return Ok(());
    }

    // Skip excluded paths
    if config.is_excluded(path) {
        return Ok(());
    }

    match event.kind {
        EventKind::Create | EventKind::Modify => {
            let content = std::fs::read_to_string(path)?;
            host.set_file(path, &content);
        }
        EventKind::Remove => {
            host.remove_file(path);
        }
        _ => return Ok(()),
    }

    // Get diagnostics for changed file and dependents
    let analysis = host.analysis();
    let diagnostics = analysis.file_diagnostics(path);

    print_file_result(path, &diagnostics, config);

    // Notify if enabled
    if config.notify && has_errors(&diagnostics) {
        send_notification(path, &diagnostics)?;
    }

    Ok(())
}
```

### Desktop Notifications

```rust
// Optional: compile with --features notify
#[cfg(feature = "notify")]
fn send_notification(path: &Path, diagnostics: &[Diagnostic]) -> Result<()> {
    use notify_rust::Notification;

    let error_count = diagnostics.iter()
        .filter(|d| d.severity == Severity::Error)
        .count();

    Notification::new()
        .summary("GraphQL Validation Error")
        .body(&format!(
            "{} error{} in {}",
            error_count,
            if error_count == 1 { "" } else { "s" },
            path.file_name().unwrap().to_string_lossy()
        ))
        .icon("dialog-error")
        .show()?;

    Ok(())
}
```

### Debouncing Strategy

```rust
// Debounce rapid changes (e.g., from editor auto-save)
const DEBOUNCE_DELAY: Duration = Duration::from_millis(100);

// Batch related changes (e.g., schema + dependent docs)
const BATCH_WINDOW: Duration = Duration::from_millis(50);

// After schema changes, wait before validating dependents
// (allows related saves to complete)
const SCHEMA_CHANGE_DELAY: Duration = Duration::from_millis(200);
```

## Integration Points

### tmux Status Line

```bash
# .tmux.conf
set -g status-right '#(graphql watch --format=status 2>/dev/null || echo "●")'
```

Status format output:
```
✓ 45 files  # All valid
● 2 errors  # Has errors
```

### Terminal Title

```bash
graphql watch --title
```

Sets terminal title to:
- `✓ graphql-lsp` when valid
- `● graphql-lsp (2 errors)` when invalid

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Clean exit (Ctrl+C) |
| 1 | Validation errors at exit |
| 2 | Configuration error |
| 3 | Watch error (permissions, etc.) |

## Configuration File Support

```yaml
# .graphqlrc.yaml
watch:
  debounce: 100  # ms
  notify: true
  include:
    - "src/**/*.graphql"
    - "src/**/*.{ts,tsx}"
  exclude:
    - "**/*.test.graphql"
    - "**/generated/**"
```

## Open Questions

1. **Config file changes**: Restart or hot reload?
   - Hot reload is complex (schema path changes)
   - Restart is simpler, recommend `--restart-on-config`

2. **Schema introspection**: How to handle remote schemas?
   - Poll interval? Manual refresh?
   - Watch mode probably shouldn't auto-refresh remote

3. **Large projects**: Performance with 1000+ files?
   - May need lazy loading
   - Only validate open/changed files?

4. **Socket/IPC interface**: For tool integration?
   - Could expose diagnostics via Unix socket
   - Would enable custom editor integrations

5. **Clear screen**: On each change?
   - Some prefer continuous scroll
   - Others prefer clean slate
   - Make configurable with `--clear`

## Next Steps

1. [ ] Add `notify` dependency
2. [ ] Implement basic watch command
3. [ ] Add pretty output formatting
4. [ ] Add JSON/JSONL output
5. [ ] Add debouncing
6. [ ] Add desktop notifications (optional feature)
7. [ ] Add filtering options
8. [ ] Test on macOS, Linux, Windows
9. [ ] Add documentation

## References

- [notify crate](https://docs.rs/notify/latest/notify/)
- [notify-rust](https://docs.rs/notify-rust/latest/notify_rust/)
- [crossterm](https://docs.rs/crossterm/latest/crossterm/)
- [similar watch implementations](https://github.com/watchexec/watchexec)
