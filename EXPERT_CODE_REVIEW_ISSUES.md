# Expert Code Review Issues

This document contains issues identified during the comprehensive SME expert code review. Each issue includes thorough description, reproduction steps, and proposed test cases.

**Labels for all issues**: `claude`

---

## Table of Contents

### Critical Issues

1. [UTF-16 Position Handling Incorrect](#issue-1-utf-16-position-handling-incorrect)
2. [Panic on Unsaved Files (untitled:// URIs)](#issue-2-panic-on-unsaved-files)
3. [apollo-compiler Error Positions Ignored](#issue-3-apollo-compiler-error-positions-ignored)
4. [RwLock Poisoning Causes Cascading Panics](#issue-4-rwlock-poisoning-causes-cascading-panics)
5. [Thread-Safety Violation in Analysis Snapshots](#issue-5-thread-safety-violation-in-analysis-snapshots)
6. [Path Traversal Vulnerability in Glob Patterns](#issue-6-path-traversal-vulnerability-in-glob-patterns)

### High Priority Issues

7. [Early Return on Parse Errors Masks Fragment Collection](#issue-7-early-return-on-parse-errors-masks-fragment-collection)
8. [Client Capabilities Ignored During Initialization](#issue-8-client-capabilities-ignored-during-initialization)
9. [No Request Cancellation Support](#issue-9-no-request-cancellation-support)
10. [No Diagnostics Debouncing](#issue-10-no-diagnostics-debouncing)
11. [Anonymous Operation Lint Rule Violates GraphQL Spec](#issue-11-anonymous-operation-lint-rule-violates-graphql-spec)
12. [Missing Interface Implementation Validation](#issue-12-missing-interface-implementation-validation)
13. [Missing Union Member Validation](#issue-13-missing-union-member-validation)
14. [VSCode Extension Missing onLanguage Activation](#issue-14-vscode-extension-missing-onlanguage-activation)
15. [VSCode Extension Over-Broad Document Selector](#issue-15-vscode-extension-over-broad-document-selector)
16. [Fragment Spreads Index Over-Invalidation](#issue-16-fragment-spreads-index-over-invalidation)
17. [Eager AST Cloning in Project Lints](#issue-17-eager-ast-cloning-in-project-lints)
18. [No HTTP Timeout for Remote Schema Loading](#issue-18-no-http-timeout-for-remote-schema-loading)

---

## Critical Issues

---

## Issue 1: UTF-16 Position Handling Incorrect

**Labels**: `bug`, `critical`, `lsp`, `claude`

### Summary

The LSP server treats character positions as byte offsets, but the LSP specification requires UTF-16 code units. This causes all position-based features (hover, goto definition, diagnostics, completions) to report incorrect positions on files containing non-ASCII characters.

### Affected Files

- `crates/graphql-lsp/src/conversions.rs:12-14`
- `crates/graphql-ide/src/lib.rs:2213-2227` (`position_to_offset`)
- `crates/graphql-syntax/src/lib.rs:380-418` (`LineIndex`)

### Problem Details

#### Current Implementation

```rust
// conversions.rs:12-14 - Direct copy without conversion
pub const fn convert_lsp_position(pos: Position) -> graphql_ide::Position {
    graphql_ide::Position::new(pos.line, pos.character)  // character is UTF-16 units!
}

// lib.rs:2213-2227 - Treats character as byte offset
fn position_to_offset(line_index: &graphql_syntax::LineIndex, position: Position) -> Option<usize> {
    let line_start = line_index.line_start(position.line as usize)?;
    Some(line_start + position.character as usize)  // BUG: direct byte addition
}
```

#### LSP Specification Requirement

From [LSP Spec Section 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position):

> Position in a text document expressed as zero-based line and **character offset**. A character offset is the gap between `chr` and `chr + 1`, **measured in UTF-16 code units**.

### Impact

- Hover shows information for wrong symbol
- Goto definition jumps to wrong location
- Diagnostics highlight wrong text
- Completions triggered at wrong positions
- Any file with emoji, CJK characters, or other non-BMP Unicode breaks

### Reproduction Steps

#### Test Case 1: Emoji in GraphQL file

Create a test file `emoji.graphql`:

```graphql
# 🚀 Launch query
query GetUser {
  user {
    name
  }
}
```

1. Open the file in VSCode with the GraphQL LSP extension
2. Position cursor on `user` field (line 3, after the emoji comment)
3. Trigger hover (Ctrl+K Ctrl+I)
4. **Expected**: Hover shows `user` field type information
5. **Actual**: Hover shows wrong information or nothing (position is off by 2 bytes for each emoji)

#### Test Case 2: CJK Characters

Create a test file `cjk.graphql`:

```graphql
# 用户查询
query GetUser {
  user {
    name # 名前
  }
}
```

1. Position cursor on `name` field
2. Trigger goto definition
3. **Expected**: Jumps to schema definition of `name`
4. **Actual**: Position calculation is off by `(CJK_char_count * 2)` bytes

### Proposed Test Case

Add to `crates/graphql-ide/src/lib.rs`:

```rust
#[cfg(test)]
mod utf16_tests {
    use super::*;

    #[test]
    fn test_utf16_position_with_emoji() {
        // Emoji (🚀) is a surrogate pair: 2 UTF-16 code units, 4 UTF-8 bytes
        let content = "# 🚀\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(content);

        // Line 0: "# 🚀" = 2 + 4 = 6 bytes, but 2 + 2 = 4 UTF-16 code units
        // Line 1: "query { user }"
        // LSP position (1, 8) should point to 'u' in 'user'

        let lsp_position = Position { line: 1, character: 8 };
        let offset = position_to_offset(&line_index, lsp_position);

        // Line 0 is 6 bytes + newline = 7 bytes
        // "query { " = 8 bytes
        // Total: 7 + 8 = 15 bytes to 'u'
        assert_eq!(offset, Some(15));
    }

    #[test]
    fn test_utf16_position_with_cjk() {
        // CJK characters: 1 UTF-16 code unit each, 3 UTF-8 bytes each
        let content = "# 你好\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(content);

        // Line 0: "# 你好" = 2 + 6 = 8 bytes, but 2 + 2 = 4 UTF-16 code units
        // Line 1: "query { user }"
        // LSP position (1, 8) should point to 'u' in 'user'

        let lsp_position = Position { line: 1, character: 8 };
        let offset = position_to_offset(&line_index, lsp_position);

        // Line 0 is 8 bytes + newline = 9 bytes
        // "query { " = 8 bytes
        // Total: 9 + 8 = 17 bytes to 'u'
        assert_eq!(offset, Some(17));
    }

    #[test]
    fn test_offset_to_utf16_position() {
        let content = "# 🚀\nquery { user }";
        let line_index = graphql_syntax::LineIndex::new(content);

        // Byte offset 15 should convert to LSP position (1, 8)
        let byte_offset = 15; // 'u' in 'user'
        let lsp_pos = offset_to_position(&line_index, byte_offset);

        assert_eq!(lsp_pos.line, 1);
        assert_eq!(lsp_pos.character, 8); // UTF-16 code units, not bytes
    }
}
```

### Proposed Solution

Add conversion functions in `conversions.rs`:

```rust
/// Convert UTF-16 code unit offset to byte offset within a line
fn utf16_to_byte_offset(line_text: &str, utf16_offset: u32) -> usize {
    let mut byte_offset = 0;
    let mut utf16_count = 0;

    for c in line_text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }
        utf16_count += c.len_utf16() as u32;
        byte_offset += c.len_utf8();
    }

    byte_offset
}

/// Convert byte offset to UTF-16 code unit offset within a line
fn byte_to_utf16_offset(line_text: &str) -> u32 {
    line_text.chars().map(|c| c.len_utf16() as u32).sum()
}

pub fn convert_lsp_position_with_content(
    pos: Position,
    content: &str,
    line_index: &LineIndex,
) -> Option<usize> {
    let line_start = line_index.line_start(pos.line as usize)?;
    let line_end = line_index.line_start(pos.line as usize + 1)
        .unwrap_or(content.len());
    let line_text = &content[line_start..line_end];

    let byte_offset_in_line = utf16_to_byte_offset(line_text, pos.character);
    Some(line_start + byte_offset_in_line)
}
```

### References

- [LSP Specification - Position](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position)
- [rust-analyzer's line_index.rs](https://github.com/rust-lang/rust-analyzer/blob/master/crates/ide/src/line_index.rs)

---

## Issue 2: Panic on Unsaved Files

**Labels**: `bug`, `critical`, `lsp`, `claude`

### Summary

The LSP server panics when handling files with `untitled://` URIs (unsaved files in editors). The panic occurs in tracing instrumentation spans where `uri.to_file_path().unwrap()` is called.

### Affected Files

- `crates/graphql-lsp/src/server.rs:501`
- `crates/graphql-lsp/src/server.rs:538`
- `crates/graphql-lsp/src/server.rs:725`
- `crates/graphql-lsp/src/server.rs:766`
- `crates/graphql-lsp/src/server.rs:808`

### Problem Details

```rust
// Line 501 - Panics before method body executes!
#[tracing::instrument(skip(self), fields(path = ?uri.to_file_path().unwrap()))]
async fn validate_file(&self, uri: Uri) {
    // ...
}

// Line 725
#[tracing::instrument(skip(self, params), fields(path = ?params.text_document.uri.to_file_path().unwrap()))]
async fn did_open(&self, params: DidOpenTextDocumentParams) {
    // ...
}
```

The `unwrap()` is called during span construction, not in error handling code. If a URI can't be converted to a file path (e.g., `untitled:Untitled-1`), the tracing span construction panics before the method body even executes.

### Impact

- **Complete LSP crash** when user creates new unsaved file
- **Complete LSP crash** when user opens file from virtual filesystem
- Users must restart editor/LSP to recover
- Common workflow (Cmd+N for new file) causes crash

### Reproduction Steps

1. Open VSCode with the GraphQL LSP extension active
2. Press `Cmd+N` (or `Ctrl+N`) to create a new unsaved file
3. Set the language mode to GraphQL (click language indicator in status bar)
4. Start typing GraphQL content
5. **Expected**: LSP provides diagnostics and features
6. **Actual**: LSP crashes immediately when `did_open` is called with `untitled:Untitled-1` URI

### Proposed Test Case

Add to `crates/graphql-lsp/src/server.rs`:

```rust
#[cfg(test)]
mod uri_handling_tests {
    use super::*;
    use lsp_types::Uri;

    #[test]
    fn test_untitled_uri_does_not_panic() {
        // This URI format is sent by VSCode for unsaved files
        let uri: Uri = "untitled:Untitled-1".parse().unwrap();

        // This should NOT panic
        let file_path = uri.to_file_path();
        assert!(file_path.is_err(), "untitled URIs should not convert to file paths");
    }

    #[test]
    fn test_virtual_uri_does_not_panic() {
        // This URI format might be sent for virtual files
        let uri: Uri = "vscode-vfs://github/user/repo/file.graphql".parse().unwrap();

        let file_path = uri.to_file_path();
        assert!(file_path.is_err(), "virtual URIs should not convert to file paths");
    }

    #[tokio::test]
    async fn test_did_open_with_untitled_uri() {
        // Integration test: did_open should handle untitled URIs gracefully
        let server = create_test_server();

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: "untitled:Untitled-1".parse().unwrap(),
                language_id: "graphql".to_string(),
                version: 1,
                text: "query { user { id } }".to_string(),
            },
        };

        // This should NOT panic, should log warning and return gracefully
        server.did_open(params).await;
    }
}
```

### Proposed Solution

Replace all panicking tracing instrumentation with safe alternatives:

```rust
// Option 1: Log the URI string directly (safest)
#[tracing::instrument(skip(self), fields(uri = %uri.as_str()))]
async fn validate_file(&self, uri: Uri) {
    // Early return for non-file URIs
    let Some(path) = uri.to_file_path().ok() else {
        tracing::debug!("Skipping non-file URI: {}", uri);
        return;
    };
    // ... rest of method
}

// Option 2: Use Option in tracing field
#[tracing::instrument(skip(self), fields(path = ?uri.to_file_path().ok()))]
async fn validate_file(&self, uri: Uri) {
    // ...
}
```

### References

- [VSCode URI Schemes](https://code.visualstudio.com/api/extension-guides/virtual-documents)
- [tower-lsp URI handling](https://docs.rs/lsp-types/latest/lsp_types/struct.Url.html)

---

## Issue 3: apollo-compiler Error Positions Ignored

**Labels**: `bug`, `critical`, `validation`, `claude`

### Summary

When parsing with apollo-compiler, the code uses `offset: 0` for all parse errors, ignoring the actual position information available from `line_column_range()`. This causes all validation errors to appear at the wrong location.

### Affected Files

- `crates/graphql-syntax/src/lib.rs:206-219`
- `crates/graphql-syntax/src/lib.rs:281-293`

### Problem Details

```rust
// Current code (WRONG) - Lines 206-219
Err(with_errors) => {
    // Collect parse errors from apollo-compiler
    // Note: apollo-compiler errors don't have precise positions in the same way,
    // so we use offset 0 for these
    errors.extend(with_errors.errors.iter().map(|e| ParseError {
        message: e.to_string(),
        offset: 0,  // ❌ IGNORING apollo-compiler's position info!
    }));
    with_errors.partial
}
```

The comment is incorrect - apollo-compiler's `Diagnostic` type DOES provide `line_column_range()` which returns accurate 1-based line/column positions.

### Impact

- All validation errors appear at line 1, column 1
- Users can't locate actual error positions
- Clicking on diagnostics doesn't jump to the problem
- Debugging schema/document errors is extremely difficult

### Reproduction Steps

1. Create a GraphQL file with a syntax error NOT at the beginning:

```graphql
type Query {
  user: User
}

type User {
  id: ID!
  name String  # Missing colon - error should be here!
}
```

2. Open in VSCode with GraphQL LSP
3. Observe that the error diagnostic appears at line 1, column 1
4. **Expected**: Error appears on line 7 where the actual syntax error is
5. **Actual**: Error appears at the start of the file

### Proposed Test Case

Add to `crates/graphql-syntax/src/lib.rs`:

```rust
#[cfg(test)]
mod error_position_tests {
    use super::*;

    #[test]
    fn test_apollo_compiler_error_has_position() {
        let content = r#"
type Query {
  user: User
}

type User {
  id: ID!
  name String
}
"#;

        // Parse with apollo-compiler
        let result = apollo_compiler::ast::Document::parse(content, "test.graphql");

        match result {
            Ok(_) => panic!("Expected parse error"),
            Err(with_errors) => {
                assert!(!with_errors.errors.is_empty(), "Should have errors");

                let first_error = &with_errors.errors[0];
                let range = first_error.line_column_range();

                assert!(range.is_some(), "apollo-compiler DOES provide position info!");

                let range = range.unwrap();
                // Error should be on line 8 (1-indexed), not line 1
                assert!(range.start.line >= 7, "Error should be near line 8, got line {}", range.start.line);
            }
        }
    }

    #[test]
    fn test_parse_error_preserves_position() {
        let content = "type Query { invalid syntax here }";
        let parse = parse_pure_graphql(content, FileKind::Schema);

        assert!(!parse.errors.is_empty(), "Should have parse errors");

        // The error offset should NOT be 0 for errors in the middle of the file
        let error = &parse.errors[0];
        assert!(error.offset > 0, "Error offset should not be 0, got {}", error.offset);
    }
}
```

### Proposed Solution

Extract position information from apollo-compiler diagnostics:

```rust
Err(with_errors) => {
    for diag in with_errors.errors.iter() {
        let offset = if let Some(range) = diag.line_column_range() {
            // apollo-compiler uses 1-based indexing
            // Convert to byte offset using LineIndex
            let line = range.start.line.saturating_sub(1);
            let col = range.start.column.saturating_sub(1);

            // Build a temporary LineIndex to convert line/col to byte offset
            let line_index = LineIndex::new(content);
            line_index.line_start(line)
                .map(|start| start + col)
                .unwrap_or(0)
        } else {
            0
        };

        errors.push(ParseError {
            message: diag.error.to_string(),
            offset,
        });
    }
    with_errors.partial
}
```

### References

- [apollo-compiler Diagnostic](https://docs.rs/apollo-compiler/latest/apollo_compiler/struct.Diagnostic.html)
- [apollo-compiler line_column_range](https://docs.rs/apollo-compiler/latest/apollo_compiler/struct.Diagnostic.html#method.line_column_range)

---

## Issue 4: RwLock Poisoning Causes Cascading Panics

**Labels**: `bug`, `critical`, `concurrency`, `claude`

### Summary

The codebase uses `std::sync::RwLock` with `.unwrap()` on lock acquisition. If any thread panics while holding the lock, the lock becomes "poisoned" and all subsequent lock attempts will panic, causing a cascading failure that brings down the entire LSP.

### Affected Files

- `crates/graphql-ide/src/lib.rs` - 20+ locations with `.unwrap()` on RwLock
- Lines: 535, 546, 568, 595, 602, 608, 611, 717, 770, 806, 845, 887, 909, 1054, 1198, 1274, 1353, 1394

### Problem Details

```rust
// Current pattern throughout the codebase
pub fn add_file(&mut self, path: &FilePath, content: &str, kind: FileKind, line_offset: u32) -> bool {
    let mut registry = self.registry.write().unwrap();  // ❌ Panics if lock poisoned
    // ...
}

pub fn snapshot(&self) -> Analysis {
    let project_files = self.registry.read().unwrap();  // ❌ Panics if lock poisoned
    // ...
}
```

### Impact

Cascading failure scenario:

1. Thread A holds write lock on registry
2. Thread A panics (e.g., from another `.unwrap()` call)
3. Lock becomes poisoned
4. Thread B tries to read registry → panics
5. Thread C tries to read registry → panics
6. LSP becomes completely unresponsive

### Reproduction Steps

This is difficult to reproduce directly but can be simulated:

```rust
#[test]
fn test_lock_poisoning_cascade() {
    use std::sync::{Arc, RwLock};
    use std::thread;

    let lock = Arc::new(RwLock::new(0));
    let lock2 = lock.clone();

    // Thread that panics while holding lock
    let handle = thread::spawn(move || {
        let _guard = lock2.write().unwrap();
        panic!("Simulated panic while holding lock");
    });

    let _ = handle.join(); // Thread panicked

    // This will panic with "poisoned lock"
    let result = std::panic::catch_unwind(|| {
        let _guard = lock.read().unwrap();
    });

    assert!(result.is_err(), "Lock should be poisoned");
}
```

### Proposed Test Case

Add to `crates/graphql-ide/src/lib.rs`:

```rust
#[cfg(test)]
mod lock_safety_tests {
    use super::*;
    use std::panic;

    #[test]
    fn test_registry_survives_panic() {
        let mut host = AnalysisHost::new();
        host.add_file(
            &FilePath::new("test.graphql"),
            "type Query { id: ID }",
            FileKind::Schema,
            0,
        );

        // Simulate a panic during some operation
        let host_clone = host.clone(); // This would need AnalysisHost: Clone
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            // Some operation that might panic
            let _snapshot = host_clone.snapshot();
            panic!("Simulated panic");
        }));

        assert!(result.is_err(), "Should have panicked");

        // Host should still be usable after panic
        let snapshot = host.snapshot();
        assert!(snapshot.diagnostics(&FilePath::new("test.graphql")).is_some());
    }
}
```

### Proposed Solution

#### Option A: Switch to parking_lot::RwLock (Recommended)

`parking_lot::RwLock` never poisons and is faster:

```rust
// In Cargo.toml
[dependencies]
parking_lot = "0.12"

// In code
use parking_lot::RwLock;

pub fn add_file(&mut self, ...) -> bool {
    let mut registry = self.registry.write();  // No .unwrap() needed
    // ...
}
```

**Tradeoffs**:

- ✅ Zero-cost abstraction, never poisons
- ✅ Faster than std::sync::RwLock
- ✅ Used by rust-analyzer
- ⚠️ Adds a dependency (already common in Rust ecosystem)

#### Option B: Handle Poisoning Gracefully

```rust
pub fn add_file(&mut self, ...) -> bool {
    let mut registry = match self.registry.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!("Registry lock poisoned, recovering...");
            poisoned.into_inner()  // Recover from poisoned lock
        }
    };
    // ...
}
```

**Tradeoffs**:

- ✅ No new dependencies
- ⚠️ More verbose code
- ⚠️ May hide underlying bugs that caused the poison

### References

- [parking_lot crate](https://docs.rs/parking_lot/latest/parking_lot/)
- [rust-analyzer uses parking_lot](https://github.com/rust-lang/rust-analyzer/blob/master/Cargo.toml)
- [std::sync::RwLock poisoning](https://doc.rust-lang.org/std/sync/struct.RwLock.html#poisoning)

---

## Issue 5: Thread-Safety Violation in Analysis Snapshots

**Labels**: `bug`, `critical`, `architecture`, `claude`

### Summary

The `Analysis` struct stores `Arc<RwLock<FileRegistry>>` which can be modified by `AnalysisHost` after snapshot creation. This violates the architectural guarantee that snapshots are immutable and thread-safe.

### Affected Files

- `crates/graphql-ide/src/lib.rs:756-762`

### Problem Details

```rust
#[derive(Clone)]
pub struct Analysis {
    db: IdeDatabase,
    registry: Arc<RwLock<FileRegistry>>,  // ← Can be modified by AnalysisHost!
    project_files: Option<graphql_db::ProjectFiles>,  // ← Stale snapshot
}
```

The `Analysis` struct is supposed to be an immutable snapshot per the rust-analyzer pattern, but:

1. `registry` is shared with `AnalysisHost` via `Arc`
2. `AnalysisHost` can modify `registry` after `Analysis` is created
3. Concurrent queries on `Analysis` may see different registry states
4. `project_files` becomes stale when `AnalysisHost` adds/removes files

### Impact

- Race conditions in concurrent queries
- Queries may return inconsistent results
- "File not found" errors for files that were just added
- Stale diagnostics for removed files

### Reproduction Steps

```rust
#[test]
fn test_snapshot_isolation_violation() {
    let mut host = AnalysisHost::new();
    host.add_file(&FilePath::new("a.graphql"), "type A { id: ID }", FileKind::Schema, 0);

    let snapshot = host.snapshot();

    // Add file AFTER snapshot was taken
    host.add_file(&FilePath::new("b.graphql"), "type B { id: ID }", FileKind::Schema, 0);

    // Query on snapshot - what happens?
    // Currently: might see file B (violated isolation!)
    // Expected: should NOT see file B

    let files = snapshot.all_files(); // What does this return?
}
```

### Proposed Test Case

Add to `crates/graphql-ide/src/lib.rs`:

```rust
#[cfg(test)]
mod snapshot_isolation_tests {
    use super::*;

    #[test]
    fn test_snapshot_does_not_see_later_additions() {
        let mut host = AnalysisHost::new();
        host.add_file(
            &FilePath::new("a.graphql"),
            "type Query { a: String }",
            FileKind::Schema,
            0,
        );

        let snapshot1 = host.snapshot();

        // Add another file after snapshot
        host.add_file(
            &FilePath::new("b.graphql"),
            "type Query { b: String }",
            FileKind::Schema,
            0,
        );

        let snapshot2 = host.snapshot();

        // snapshot1 should NOT see b.graphql
        assert!(
            snapshot1.file_diagnostics(&FilePath::new("b.graphql")).is_none(),
            "snapshot1 should not see files added after it was created"
        );

        // snapshot2 SHOULD see b.graphql
        assert!(
            snapshot2.file_diagnostics(&FilePath::new("b.graphql")).is_some(),
            "snapshot2 should see b.graphql"
        );
    }

    #[test]
    fn test_snapshot_does_not_see_modifications() {
        let mut host = AnalysisHost::new();
        host.add_file(
            &FilePath::new("a.graphql"),
            "type Query { original: String }",
            FileKind::Schema,
            0,
        );

        let snapshot1 = host.snapshot();

        // Modify the file
        host.update_file(&FilePath::new("a.graphql"), "type Query { modified: String }");

        let snapshot2 = host.snapshot();

        // Queries on snapshot1 should return results based on "original" content
        // Queries on snapshot2 should return results based on "modified" content
        // Currently this may not be true!
    }
}
```

### Proposed Solution

#### Option A: Clone FileRegistry into Snapshots

```rust
pub fn snapshot(&self) -> Analysis {
    let registry_snapshot = self.registry.read().unwrap().clone();

    Analysis {
        db: self.db.clone(),
        registry: Arc::new(RwLock::new(registry_snapshot)),  // Isolated copy
        project_files: self.project_files.clone(),
    }
}
```

**Tradeoffs**:

- ✅ True isolation
- ⚠️ Memory overhead (copies all file data)
- ⚠️ Clone must be implemented for FileRegistry

#### Option B: Immutable FileRegistry with Arc Swap

```rust
pub struct AnalysisHost {
    registry: Arc<FileRegistry>,  // Immutable, replaced on changes
    // ...
}

impl AnalysisHost {
    pub fn add_file(&mut self, ...) {
        let mut new_registry = (*self.registry).clone();
        new_registry.add_file(...);
        self.registry = Arc::new(new_registry);
    }

    pub fn snapshot(&self) -> Analysis {
        Analysis {
            registry: self.registry.clone(),  // Just clone the Arc
            // ...
        }
    }
}
```

**Tradeoffs**:

- ✅ True isolation
- ✅ Cheaper snapshots (just Arc clone)
- ⚠️ More expensive mutations (full clone on each change)
- ✅ This is the rust-analyzer pattern

### References

- [rust-analyzer Analysis pattern](https://rust-analyzer.github.io/book/contributing/architecture.html#analysis)
- [Persistent data structures for immutable snapshots](https://github.com/orium/rpds)

---

## Issue 6: Path Traversal Vulnerability in Glob Patterns

**Labels**: `bug`, `critical`, `security`, `claude`

### Summary

Glob patterns from configuration files are not validated against workspace boundaries. A malicious `.graphqlrc.yaml` can read arbitrary files from the filesystem.

### Affected Files

- `crates/graphql-lsp/src/server.rs:254-256`
- `crates/graphql-ide/src/lib.rs:652-655`

### Problem Details

```rust
// server.rs:254-256
let full_pattern = workspace_path.join(&expanded_pattern);
match glob::glob(&full_pattern.display().to_string()) {
    Ok(paths) => {
        for entry in paths {
            match entry {
                Ok(path) if path.is_file() => {
                    match std::fs::read_to_string(&path) {  // ← Can read ANY file!
```

There is no validation that the resolved path is within the workspace.

### Impact

- Attacker can read `/etc/passwd`, SSH keys, environment files
- Configuration file is trusted without validation
- LSP runs with user privileges

### Reproduction Steps

1. Create a malicious `.graphqlrc.yaml`:

```yaml
schema: schema.graphql
documents: "../../../../etc/passwd"
```

2. Open the project in VSCode with GraphQL LSP
3. The LSP will attempt to read `/etc/passwd` as a GraphQL document
4. While parsing will fail, the file content is read into memory

### Proposed Test Case

Add to `crates/graphql-lsp/src/server.rs`:

```rust
#[cfg(test)]
mod security_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_path_traversal_blocked() {
        let workspace = PathBuf::from("/home/user/project");
        let pattern = "../../../../etc/passwd";

        let full_pattern = workspace.join(pattern);
        let canonical = std::fs::canonicalize(&full_pattern);

        // If canonicalize succeeds, verify it's within workspace
        if let Ok(canonical_path) = canonical {
            let canonical_workspace = std::fs::canonicalize(&workspace).unwrap();
            assert!(
                canonical_path.starts_with(&canonical_workspace),
                "Path traversal should be blocked: {:?} escapes {:?}",
                canonical_path,
                canonical_workspace
            );
        }
    }

    #[test]
    fn test_glob_pattern_validation() {
        // These patterns should be rejected
        let dangerous_patterns = vec![
            "../../../etc/passwd",
            "/etc/passwd",
            "~/.ssh/id_rsa",
            "C:\\Windows\\System32\\config\\SAM",
        ];

        for pattern in dangerous_patterns {
            assert!(
                is_safe_glob_pattern(pattern, &PathBuf::from("/workspace")).is_err(),
                "Pattern should be rejected: {}",
                pattern
            );
        }
    }
}
```

### Proposed Solution

Add path validation before processing glob results:

```rust
fn validate_path_within_workspace(path: &Path, workspace: &Path) -> Result<PathBuf, String> {
    // Canonicalize both paths to resolve symlinks and ..
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|e| format!("Failed to canonicalize path: {}", e))?;
    let canonical_workspace = std::fs::canonicalize(workspace)
        .map_err(|e| format!("Failed to canonicalize workspace: {}", e))?;

    if !canonical_path.starts_with(&canonical_workspace) {
        return Err(format!(
            "Path {:?} is outside workspace {:?}",
            canonical_path, canonical_workspace
        ));
    }

    Ok(canonical_path)
}

// In the glob processing loop:
Ok(path) if path.is_file() => {
    match validate_path_within_workspace(&path, &workspace_path) {
        Ok(safe_path) => {
            match std::fs::read_to_string(&safe_path) {
                // ...
            }
        }
        Err(e) => {
            tracing::warn!("Skipping file outside workspace: {}", e);
            continue;
        }
    }
}
```

**Tradeoffs**:

- ✅ Prevents path traversal attacks
- ⚠️ May break legitimate symlinks to files outside workspace
- ⚠️ Adds filesystem syscalls for canonicalization

---

## High Priority Issues

---

## Issue 7: Early Return on Parse Errors Masks Fragment Collection

**Labels**: `bug`, `high`, `validation`, `claude`

### Summary

When apollo-parser reports errors, the code returns an empty set of fragment references instead of continuing to extract them. This causes missing cross-file fragment references even when the fragments themselves are valid.

### Affected Files

- `crates/graphql-analysis/src/validation.rs:269-272`

### Problem Details

```rust
fn collect_referenced_fragments_from_tree(
    tree: &apollo_parser::SyntaxTree,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    if tree.errors().next().is_some() {
        // If there are parse errors, return empty set
        return HashSet::new();  // ❌ WRONG: CST is still valid!
    }
    // ...
}
```

apollo-parser is **error-tolerant**: it always produces a CST even with syntax errors. The document might have valid fragment spreads even if there are parse errors elsewhere.

### Impact

- Cross-file fragment resolution fails when files have any syntax error
- False "unknown fragment" errors
- Valid fragments not validated because they're not collected

### Reproduction Steps

Create `query.graphql`:

```graphql
query GetUser {
  ...UserFragment
  invalidSyntax{   # Missing closing brace
}
```

Create `fragment.graphql`:

```graphql
fragment UserFragment on User {
  id
  name
}
```

**Expected**: Error about syntax, but `UserFragment` should still resolve
**Actual**: Both "syntax error" AND "unknown fragment UserFragment"

### Proposed Test Case

```rust
#[test]
fn test_fragment_collection_with_parse_errors() {
    let content = r#"
query GetUser {
  ...UserFragment
  invalidSyntax{
}
"#;

    let parser = apollo_parser::Parser::new(content);
    let tree = parser.parse();

    // There SHOULD be parse errors
    assert!(tree.errors().next().is_some(), "Should have parse errors");

    // But we should STILL collect fragment spreads
    let fragments = collect_referenced_fragments_from_tree(&tree);

    assert!(
        fragments.contains("UserFragment"),
        "Should still collect UserFragment despite parse errors"
    );
}
```

### Proposed Solution

Remove the early return:

```rust
fn collect_referenced_fragments_from_tree(
    tree: &apollo_parser::SyntaxTree,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    // Always attempt to collect, regardless of errors
    // apollo-parser produces a usable CST even with syntax errors
    let mut referenced = HashSet::new();
    let document = tree.document();

    for definition in document.definitions() {
        match definition {
            apollo_parser::cst::Definition::OperationDefinition(op) => {
                collect_fragment_spreads_from_selection_set(op.selection_set(), &mut referenced);
            }
            apollo_parser::cst::Definition::FragmentDefinition(frag) => {
                collect_fragment_spreads_from_selection_set(frag.selection_set(), &mut referenced);
            }
            _ => {}
        }
    }

    referenced
}
```

---

## Issue 8: Client Capabilities Ignored During Initialization

**Labels**: `bug`, `high`, `lsp`, `claude`

### Summary

The server receives `InitializeParams` which contains `client_capabilities`, but never uses them. The server returns static capabilities without negotiating based on what the client actually supports.

### Affected Files

- `crates/graphql-lsp/src/server.rs:559-607`

### Problem Details

```rust
async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    // params.capabilities is completely ignored!

    Ok(InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::FULL,  // Always FULL, never checked client
            )),
            // ... other capabilities always enabled
        },
    })
}
```

### Impact

- Clients that don't support certain features may behave incorrectly
- No graceful degradation for limited LSP clients
- Cannot detect if client supports workspace symbol batching

### Proposed Test Case

```rust
#[tokio::test]
async fn test_respects_client_capabilities() {
    let server = create_test_server();

    // Client that doesn't support semantic tokens
    let params = InitializeParams {
        capabilities: ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                semantic_tokens: None,  // Client doesn't support this
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let result = server.initialize(params).await.unwrap();

    // Server should NOT advertise semantic tokens if client doesn't support it
    assert!(
        result.capabilities.semantic_tokens_provider.is_none(),
        "Should not advertise unsupported features"
    );
}
```

### Proposed Solution

Store and use client capabilities:

```rust
struct GraphQLLanguageServer {
    client_capabilities: RwLock<Option<ClientCapabilities>>,
    // ...
}

async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    // Store client capabilities
    *self.client_capabilities.write().await = Some(params.capabilities.clone());

    // Build server capabilities based on client support
    let supports_semantic_tokens = params.capabilities
        .text_document
        .as_ref()
        .and_then(|td| td.semantic_tokens.as_ref())
        .is_some();

    Ok(InitializeResult {
        capabilities: ServerCapabilities {
            semantic_tokens_provider: if supports_semantic_tokens {
                Some(/* ... */)
            } else {
                None
            },
            // ...
        },
    })
}
```

---

## Issue 9: No Request Cancellation Support

**Labels**: `enhancement`, `high`, `lsp`, `claude`

### Summary

The LSP server doesn't handle `$/cancelRequest` protocol messages. If a client cancels a request (user switches focus before hover completes), the server continues computing wastefully.

### Affected Files

- `crates/graphql-lsp/src/server.rs` (entire file)

### Problem Details

```rust
// No cancellation handling anywhere in the codebase
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    // No way to check if request was cancelled
    // Continues computing even if user moved away
}
```

### Impact

- Wasted CPU cycles on cancelled requests
- Poor responsiveness - old requests block new ones
- Resource exhaustion under rapid user actions

### Proposed Solution

#### Option A: Add Timeouts (Quick Win)

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    tokio::time::timeout(Duration::from_millis(500), async {
        // Perform analysis
    })
    .await
    .unwrap_or(Ok(None))
}
```

#### Option B: Cooperative Cancellation

Integrate with Salsa's cancellation support and tower-lsp's request tracking.

---

## Issue 10: No Diagnostics Debouncing

**Labels**: `enhancement`, `high`, `lsp`, `performance`, `claude`

### Summary

Diagnostics are published immediately after every change with no debouncing. When a user types quickly, this causes diagnostic flickering and wasted computation.

### Affected Files

- `crates/graphql-lsp/src/server.rs:502-553`
- `crates/graphql-lsp/src/server.rs:767-805`

### Problem Details

```rust
async fn did_change(&self, params: DidChangeTextDocumentParams) {
    for change in params.content_changes {
        // Validate immediately on every keystroke
        self.validate_file_with_snapshot(&uri, snapshot).await;
    }
}
```

### Impact

- Diagnostics panel flickers during typing
- CPU spikes on every keystroke
- Wasted validation of incomplete code

### Proposed Solution

```rust
struct PendingValidation {
    content: String,
    timer: Option<tokio::time::Sleep>,
}

async fn did_change(&self, params: DidChangeTextDocumentParams) {
    // Store pending change
    self.pending_validations.insert(uri.clone(), content);

    // Debounce: wait 300ms before validating
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Check if content is still the pending one
        if self.pending_validations.get(&uri) == Some(&content) {
            self.validate_file(&uri).await;
        }
    });
}
```

---

## Issue 11: Anonymous Operation Lint Rule Violates GraphQL Spec

**Labels**: `bug`, `high`, `spec-compliance`, `claude`

### Summary

The `no_anonymous_operations` lint rule rejects ALL anonymous operations, but the GraphQL specification explicitly allows a single anonymous operation per document.

### Affected Files

- `crates/graphql-linter/src/rules/no_anonymous_operations.rs`

### Problem Details

**Spec Reference**: [GraphQL Spec Section 5.2.1](https://spec.graphql.org/June2018/#sec-Executable-Definitions)

> "An executable document is only valid if it contains at most one anonymous operation definition"

This means:

- ✅ Document with 1 anonymous operation: **VALID**
- ❌ Document with 2+ operations where any is anonymous: Invalid

The current rule rejects ALL anonymous operations, even single ones.

### Reproduction Steps

```graphql
# This is VALID GraphQL per the spec!
{
  user {
    id
    name
  }
}
```

**Expected**: No errors (valid GraphQL)
**Actual**: Error "Operations should be named"

### Proposed Test Case

```rust
#[test]
fn test_single_anonymous_operation_is_valid() {
    let doc = r#"
{
  user {
    id
  }
}
"#;

    let diagnostics = run_lint(doc);

    // Per GraphQL spec, a single anonymous operation is VALID
    assert!(
        diagnostics.is_empty(),
        "Single anonymous operation should be valid per spec"
    );
}

#[test]
fn test_multiple_operations_with_anonymous_is_invalid() {
    let doc = r#"
{
  user { id }
}

query GetPosts {
  posts { id }
}
"#;

    let diagnostics = run_lint(doc);

    // Multiple operations with anonymous is invalid
    assert!(
        !diagnostics.is_empty(),
        "Anonymous operation with other operations should error"
    );
}
```

### Proposed Solution

Update the rule to only flag anonymous operations when:

1. There are multiple operations in the document, AND
2. At least one is anonymous

Consider adding a separate **optional** rule `require_named_operations` for stricter policies.

---

## Issue 12: Missing Interface Implementation Validation

**Labels**: `bug`, `high`, `spec-compliance`, `validation`, `claude`

### Summary

Interface implementation validation is not working. Tests are marked as ignored with the note "apollo-compiler SchemaBuilder is lenient".

### Affected Files

- `crates/graphql-analysis/src/schema_validation.rs:135-189`

### Problem Details

```rust
#[test]
#[ignore = "Interface implementation validation requires merged schema"]
fn test_interface_implementation_missing_field() { ... }

#[test]
#[ignore = "Interface implementation validation requires merged schema"]
fn test_interface_implementation_wrong_type() { ... }
```

**Spec Reference**: [GraphQL Spec Section 3.1.2.2](https://spec.graphql.org/June2018/#sec-Interfaces)

### Reproduction Steps

This schema should produce errors but doesn't:

```graphql
interface Node {
  id: ID!
  name: String!
}

type User implements Node {
  id: ID!
  # MISSING: name field - SPEC VIOLATION
}

type Post implements Node {
  id: Int! # WRONG TYPE - SPEC VIOLATION
  name: String!
}
```

### Proposed Solution

Add post-merge validation in `merged_schema.rs`:

```rust
fn validate_interface_implementations(schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for type_def in schema.types.values() {
        if let TypeDefinition::Object(obj) = type_def {
            for interface_name in &obj.implements_interfaces {
                if let Some(interface) = schema.get_interface(interface_name) {
                    // Check all interface fields are present
                    for field in &interface.fields {
                        if !obj.fields.contains_key(&field.name) {
                            diagnostics.push(/* missing field error */);
                        }
                    }
                }
            }
        }
    }

    diagnostics
}
```

---

## Issue 13: Missing Union Member Validation

**Labels**: `bug`, `high`, `spec-compliance`, `validation`, `claude`

### Summary

Union member validation is not working. Non-object types can be added to unions without error.

### Affected Files

- `crates/graphql-analysis/src/schema_validation.rs:191-243`

### Problem Details

**Spec Reference**: [GraphQL Spec Section 3.1.5](https://spec.graphql.org/June2018/#sec-Unions)

> "A Union Type is only valid if each member type is an Object type"

### Reproduction Steps

This schema should error but doesn't:

```graphql
scalar DateTime
interface Node {
  id: ID!
}

union SearchResult = DateTime | Node # BOTH INVALID
```

### Proposed Solution

Add validation in merged schema processing.

---

## Issue 14: VSCode Extension Missing onLanguage Activation

**Labels**: `bug`, `high`, `vscode`, `claude`

### Summary

The VSCode extension only activates on workspace config presence, not when opening GraphQL files. Users opening `.graphql` files in projects without config get zero features.

### Affected Files

- `editors/vscode/package.json:15-18`

### Problem Details

```json
"activationEvents": [
  "workspaceContains:**/graphql.config.{yaml,yml,json}",
  "workspaceContains:**/.graphqlrc{.yaml,.yml,.json,}"
]
```

Missing: `"onLanguage:graphql"`

### Reproduction Steps

1. Create a new folder with just `query.graphql`
2. Open in VSCode with extension installed
3. No syntax highlighting, no hover, no validation
4. **Expected**: Basic LSP features should work
5. **Actual**: Extension never activates

### Proposed Solution

```json
"activationEvents": [
  "onLanguage:graphql",
  "workspaceContains:**/graphql.config.{yaml,yml,json}",
  "workspaceContains:**/.graphqlrc{.yaml,.yml,.json,}"
]
```

---

## Issue 15: VSCode Extension Over-Broad Document Selector

**Labels**: `bug`, `high`, `vscode`, `performance`, `claude`

### Summary

The language client is subscribed to ALL TypeScript and JavaScript files, causing massive performance overhead in large projects.

### Affected Files

- `editors/vscode/src/extension.ts:47-54`

### Problem Details

```typescript
documentSelector: [
  { scheme: "file", language: "graphql" },
  { scheme: "file", language: "typescript" },      // TOO BROAD!
  { scheme: "file", language: "typescriptreact" }, // TOO BROAD!
  { scheme: "file", language: "javascript" },      // TOO BROAD!
  { scheme: "file", language: "javascriptreact" }, // TOO BROAD!
],
```

### Impact

- LSP receives open/change events for every TS/JS file
- Memory bloat tracking all files
- CPU spikes when opening large TS projects

### Proposed Solution

Remove TS/JS from document selector. Use grammar injection for embedded GraphQL syntax highlighting only:

```typescript
documentSelector: [
  { scheme: "file", language: "graphql" },
  { scheme: "file", pattern: "**/*.{graphql,gql}" },
],
```

---

## Issue 16: Fragment Spreads Index Over-Invalidation

**Labels**: `bug`, `high`, `performance`, `architecture`, `claude`

### Summary

The `fragment_spreads_index` Salsa query rebuilds the entire project-wide index when ANY fragment file changes, violating fine-grained incremental computation principles.

### Affected Files

- `crates/graphql-hir/src/lib.rs:294-316`

### Problem Details

```rust
#[salsa::tracked]
pub fn fragment_spreads_index(...) -> Arc<HashMap<Arc<str>, HashSet<Arc<str>>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Calls fragment_body for EVERY fragment in project
        let body = fragment_body(db, content, metadata, fragment.name.clone());
        index.insert(fragment.name.clone(), body.fragment_spreads.clone());
    }
    Arc::new(index)
}
```

### Impact

- Editing fragment A rebuilds spreads for B, C, D...
- O(n) work per change instead of O(1)
- Violates structure/body separation

### Proposed Solution

Split into per-file indices:

```rust
#[salsa::tracked]
pub fn file_fragment_spreads(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<HashMap<Arc<str>, HashSet<Arc<str>>>> {
    // Only rebuilds when THIS file changes
}
```

---

## Issue 17: Eager AST Cloning in Project Lints

**Labels**: `bug`, `high`, `performance`, `claude`

### Summary

Project-wide lints clone entire ASTs for every document, causing massive memory allocation.

### Affected Files

- `crates/graphql-analysis/src/project_lints.rs:47-50`

### Problem Details

```rust
for doc in parse.documents() {
    all_documents.push(Arc::new(doc.ast.clone()));  // Clones entire AST!
}
```

For 100 files at 1MB each = 100MB cloned per lint pass.

### Proposed Solution

Use fragment index directly without AST cloning:

```rust
for doc in parse.documents() {
    // Access AST through Salsa query (cached) instead of cloning
    let fragments = file_fragments(db, file_id, content, metadata);
}
```

---

## Issue 18: No HTTP Timeout for Remote Schema Loading

**Labels**: `bug`, `high`, `security`, `claude`

### Summary

Remote schema introspection has no timeout configured. A malicious or slow endpoint can hang the LSP indefinitely.

### Affected Files

- `crates/graphql-introspect/src/query.rs:145-182`

### Problem Details

```rust
let client = reqwest::Client::new();  // No timeout!
let response = client
    .post(url)
    .json(&query_body)
    .send()  // Can hang forever
    .await
```

### Impact

- Slowloris attacks can hang LSP
- Unresponsive endpoints block initialization
- No way to recover without restarting

### Proposed Solution

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(10))
    .build()?;
```

---

## Summary

| Priority  | Count  | Categories                       |
| --------- | ------ | -------------------------------- |
| Critical  | 6      | Panics, Security, Thread Safety  |
| High      | 12     | Performance, Spec Compliance, UX |
| **Total** | **18** |                                  |

All issues should be labeled with `claude` in addition to their specific labels.
