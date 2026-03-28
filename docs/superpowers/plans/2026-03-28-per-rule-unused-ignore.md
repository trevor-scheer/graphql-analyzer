# Per-Rule Unused Ignore Diagnostics

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a `# graphql-analyzer-ignore: ruleA, ruleB` directive has some rules that matched diagnostics and some that didn't, report each unused rule individually rather than treating the directive as all-or-nothing.

**Architecture:** Replace `find_unused_directives` with `find_unused_rules` that returns per-rule granularity via an `UnusedIgnore` enum. Change `IgnoreDirective.rules` from `Vec<String>` to `Vec<RuleSpan>` to track byte offsets of individual rule names within the comment. The consumer in `lint_integration.rs` then produces specific messages and underlines the unused rule name.

**Tech Stack:** Rust, graphql-linter crate, graphql-analysis crate, Salsa

**Known limitation (pre-existing, not addressed here):** `unused_ignore_diagnostics` only considers diagnostics from `standalone_document_rules()` and `document_schema_rules()`, not `project_rules()`. A user ignoring a project-wide rule may get a false "unused" warning. This is orthogonal to the per-rule granularity change.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/linter/src/ignore.rs` | Modify | `RuleSpan` type, `UnusedIgnore` enum, `find_unused_rules` fn, updated parser |
| `crates/analysis/src/lint_integration.rs` | Modify | `unused_ignore_diagnostics` uses new API, per-rule messages + ranges |
| `crates/analysis/tests/analysis_tests.rs` | Modify | Integration tests for partial-unused |
| `test-workspace/lint-ignores/src/operations.graphql` | Modify | Fix `require_id_field` misuse, add partial-unused example |
| `docs/ignoring-lint-rules.md` | Modify | Document partial-unused behavior |

---

### Task 1: Add `RuleSpan`, `UnusedIgnore`, and `find_unused_rules` to ignore.rs

**Files:**
- Modify: `crates/linter/src/ignore.rs`

- [ ] **Step 1: Write failing tests for the new API**

Add to the existing `mod tests` block. Note: `directive()` helper uses dummy byte offsets (0, 0) for `RuleSpan` — this is fine for logic tests; integration tests in Task 2 cover real byte offsets via `parse_ignore_directives`.

```rust
#[test]
fn find_unused_rules_partial() {
    let directives = vec![directive(0, vec!["no_deprecated", "require_id_field"])];
    let diag_lines = vec![(1, "no_deprecated")];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert_eq!(unused.len(), 1);
    match &unused[0] {
        UnusedIgnore::UnusedRules { directive, rules } => {
            assert_eq!(directive.line, 0);
            let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(names, vec!["require_id_field"]);
        }
        other => panic!("Expected UnusedRules, got {other:?}"),
    }
}

#[test]
fn find_unused_rules_all_rules_unused() {
    let directives = vec![directive(0, vec!["no_deprecated", "require_id_field"])];
    let diag_lines: Vec<(usize, &str)> = vec![];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert_eq!(unused.len(), 1);
    match &unused[0] {
        UnusedIgnore::EntireDirective(d) => assert_eq!(d.line, 0),
        other => panic!("Expected EntireDirective, got {other:?}"),
    }
}

#[test]
fn find_unused_rules_all_used() {
    let directives = vec![directive(0, vec!["no_deprecated", "require_id_field"])];
    let diag_lines = vec![(1, "no_deprecated"), (1, "require_id_field")];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert!(unused.is_empty());
}

#[test]
fn find_unused_rules_bare_ignore_unused() {
    let directives = vec![directive(0, vec![])];
    let diag_lines: Vec<(usize, &str)> = vec![];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert_eq!(unused.len(), 1);
    match &unused[0] {
        UnusedIgnore::EntireDirective(d) => assert!(d.rules.is_empty()),
        other => panic!("Expected EntireDirective for bare ignore, got {other:?}"),
    }
}

#[test]
fn find_unused_rules_bare_ignore_used() {
    let directives = vec![directive(0, vec![])];
    let diag_lines = vec![(1, "any_rule")];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert!(unused.is_empty());
}

#[test]
fn find_unused_rules_single_rule_unused() {
    // Single-rule directive where the rule doesn't fire -> EntireDirective, not UnusedRules
    let directives = vec![directive(0, vec!["no_deprecated"])];
    let diag_lines: Vec<(usize, &str)> = vec![];
    let unused = find_unused_rules(&directives, &diag_lines);
    assert_eq!(unused.len(), 1);
    match &unused[0] {
        UnusedIgnore::EntireDirective(d) => {
            let names: Vec<&str> = d.rule_names();
            assert_eq!(names, vec!["no_deprecated"]);
        }
        other => panic!("Expected EntireDirective for single unused rule, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphql-linter --lib ignore 2>&1 | tail -20`
Expected: compilation errors (`RuleSpan`, `UnusedIgnore`, `find_unused_rules`, `rule_names` don't exist)

- [ ] **Step 3: Add `RuleSpan` struct and update `IgnoreDirective`**

Add `RuleSpan`:

```rust
/// Byte range of a single rule name within an ignore comment.
/// Byte offsets are file-relative (not line-relative).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpan {
    pub name: String,
    /// Byte offset of the rule name start, relative to the file.
    pub byte_offset: usize,
    /// Byte offset of the rule name end, relative to the file.
    pub byte_end: usize,
}
```

Change `IgnoreDirective.rules` from `Vec<String>` to `Vec<RuleSpan>`.

Update `suppresses()`:
```rust
pub fn suppresses(&self, rule_name: &str) -> bool {
    self.rules.is_empty() || self.rules.iter().any(|r| r.name == rule_name)
}
```

Add `rule_names()` convenience:
```rust
pub fn rule_names(&self) -> Vec<&str> {
    self.rules.iter().map(|r| r.name.as_str()).collect()
}
```

- [ ] **Step 4: Update `parse_ignore_directives` to compute per-rule byte offsets**

The tricky part: we need to find each rule name's position within the original line. The current parser does `rule_list.split(',').map(|r| r.trim())` which loses offset info.

New approach — iterate through `rule_list` tracking byte position:

```rust
} else if let Some(rule_list) = rest.strip_prefix(':') {
    // `rule_list` is a sub-slice of `line`, which starts at `line_start` in the file.
    // Compute the byte offset where `rule_list` starts within the file.
    let rule_list_offset_in_line = line.len() - rest.len() + 1; // +1 for ':'
    let rule_list_file_offset = line_start + rule_list_offset_in_line;

    let mut rules = Vec::new();
    let mut pos = 0; // current position within rule_list
    for segment in rule_list.split(',') {
        let trimmed = segment.trim();
        if !trimmed.is_empty() {
            let trim_start = segment.find(trimmed).unwrap();
            let name_start = rule_list_file_offset + pos + trim_start;
            let name_end = name_start + trimmed.len();
            rules.push(RuleSpan {
                name: trimmed.to_string(),
                byte_offset: name_start,
                byte_end: name_end,
            });
        }
        pos += segment.len() + 1; // +1 for the comma
    }

    directives.push(IgnoreDirective {
        line: line_num,
        byte_offset: line_start,
        byte_end: line_end,
        rules,
    });
}
```

- [ ] **Step 5: Add `UnusedIgnore` enum and `find_unused_rules` function**

```rust
/// Result of checking whether an ignore directive (or individual rules) is unused.
#[derive(Debug)]
pub enum UnusedIgnore<'a> {
    /// The entire directive matched nothing (bare ignore or all listed rules unused).
    EntireDirective(&'a IgnoreDirective),
    /// Some rules in a multi-rule directive were unused.
    UnusedRules {
        directive: &'a IgnoreDirective,
        rules: Vec<&'a RuleSpan>,
    },
}

#[must_use]
pub fn find_unused_rules<'a>(
    directives: &'a [IgnoreDirective],
    diagnostic_lines_and_rules: &[(usize, &str)],
) -> Vec<UnusedIgnore<'a>> {
    directives
        .iter()
        .filter_map(|d| {
            let target_line = d.line + 1;
            let has_any_diag = diagnostic_lines_and_rules
                .iter()
                .any(|(line, _)| *line == target_line);

            if d.rules.is_empty() {
                // Bare ignore: used if any diagnostic on target line
                if has_any_diag {
                    None
                } else {
                    Some(UnusedIgnore::EntireDirective(d))
                }
            } else {
                let unused_rules: Vec<&RuleSpan> = d
                    .rules
                    .iter()
                    .filter(|r| {
                        !diagnostic_lines_and_rules
                            .iter()
                            .any(|(line, rule)| *line == target_line && *rule == r.name)
                    })
                    .collect();

                if unused_rules.is_empty() {
                    None
                } else if unused_rules.len() == d.rules.len() {
                    Some(UnusedIgnore::EntireDirective(d))
                } else {
                    Some(UnusedIgnore::UnusedRules {
                        directive: d,
                        rules: unused_rules,
                    })
                }
            }
        })
        .collect()
}
```

Remove `find_unused_directives` — it has exactly one caller (`lint_integration.rs:470`) which we update in Task 2.

- [ ] **Step 6: Update `directive()` test helper and existing tests**

Update `directive()` to produce `Vec<RuleSpan>` (with dummy 0,0 offsets):

```rust
fn directive(line: usize, rules: Vec<&str>) -> IgnoreDirective {
    IgnoreDirective {
        line,
        byte_offset: 0,
        byte_end: 0,
        rules: rules
            .into_iter()
            .map(|name| RuleSpan {
                name: name.to_string(),
                byte_offset: 0,
                byte_end: 0,
            })
            .collect(),
    }
}
```

Update assertions that compare `.rules` directly against `vec!["..."]`:
- `parse_ignore_with_rules`: change to `directives[0].rule_names(), vec!["no_deprecated", "unused_variables"]`
- `parse_ignore_with_extra_whitespace`: same pattern
- `single_rule_ignore`: change to `directives[0].rule_names(), vec!["no_anonymous_operations"]`

Add a test for `RuleSpan` byte offsets from `parse_ignore_directives`:

```rust
#[test]
fn rule_span_byte_offsets_are_correct() {
    let source = "# graphql-analyzer-ignore: no_deprecated, require_id_field\nquery { hello }";
    let directives = parse_ignore_directives(source);
    assert_eq!(directives.len(), 1);
    let rules = &directives[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].name, "no_deprecated");
    assert_eq!(&source[rules[0].byte_offset..rules[0].byte_end], "no_deprecated");
    assert_eq!(rules[1].name, "require_id_field");
    assert_eq!(&source[rules[1].byte_offset..rules[1].byte_end], "require_id_field");
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p graphql-linter --lib ignore 2>&1 | tail -30`
Expected: all pass

- [ ] **Step 8: Commit**

```bash
git add crates/linter/src/ignore.rs
git commit -m "feat(linter): add per-rule unused ignore detection

Replace find_unused_directives with find_unused_rules that reports
individual unused rules within multi-rule ignore directives. Add
RuleSpan to track byte offsets of each rule name for precise
diagnostic underlines."
```

---

### Task 2: Update `unused_ignore_diagnostics` in lint_integration.rs

**Files:**
- Modify: `crates/analysis/src/lint_integration.rs`
- Modify: `crates/analysis/tests/analysis_tests.rs`

- [ ] **Step 1: Write failing integration tests**

Add to `analysis_tests.rs` after the existing ignore tests:

```rust
#[test]
fn test_unused_ignore_partial_multi_rule() {
    let db = LintTestDatabase::default();

    // no_anonymous_operations fires (anonymous query), no_deprecated does NOT fire (no deprecated fields).
    let source = "# graphql-analyzer-ignore: no_anonymous_operations, no_deprecated\nquery { user { id } }";
    let diags = lint_test_file(&db, source);

    // The anonymous operation should be suppressed
    assert!(
        !diags
            .iter()
            .any(|d| d.code.as_deref() == Some("no_anonymous_operations")),
        "no_anonymous_operations should be suppressed"
    );

    // But we should get a warning about no_deprecated being unused
    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused_ignore"))
        .collect();
    assert_eq!(
        unused.len(),
        1,
        "Expected one unused_ignore for no_deprecated, got: {diags:?}"
    );
    assert!(
        unused[0].message.contains("no_deprecated"),
        "Message should mention the unused rule name, got: {}",
        unused[0].message
    );

    // The diagnostic range should underline just "no_deprecated", not the whole comment.
    // "# graphql-analyzer-ignore: no_anonymous_operations, " is 51 chars,
    // so "no_deprecated" starts at col 51 and ends at col 64.
    assert_eq!(
        unused[0].range.start.character, 51,
        "Unused rule diagnostic should start at the rule name, got: {:?}",
        unused[0].range
    );
    assert_eq!(
        unused[0].range.end.character, 64,
        "Unused rule diagnostic should end at the rule name, got: {:?}",
        unused[0].range
    );
}

#[test]
fn test_unused_ignore_all_rules_unused_in_multi_rule() {
    let db = LintTestDatabase::default();

    // Named query, no deprecated fields -> both rules unused -> entire directive flagged
    let source =
        "# graphql-analyzer-ignore: no_anonymous_operations, no_deprecated\nquery Named { user { id } }";
    let diags = lint_test_file(&db, source);

    let unused: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused_ignore"))
        .collect();
    assert_eq!(
        unused.len(),
        1,
        "Expected one unused_ignore for entire directive, got: {diags:?}"
    );
    assert!(
        unused[0].message.contains("Unused graphql-analyzer-ignore directive"),
        "All-unused should use the whole-directive message, got: {}",
        unused[0].message
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphql-analysis --test analysis_tests test_unused_ignore_partial 2>&1 | tail -20`
Expected: FAIL (compilation error since `find_unused_directives` was removed, or wrong behavior)

- [ ] **Step 3: Update `unused_ignore_diagnostics` to use `find_unused_rules`**

Replace the call to `find_unused_directives` with `find_unused_rules` and handle both variants:

```rust
use graphql_linter::ignore::{find_unused_rules, UnusedIgnore};

let unused = find_unused_rules(&file_ignores, &diag_refs);

unused
    .into_iter()
    .flat_map(|u| match u {
        UnusedIgnore::EntireDirective(d) => {
            let (start_line, start_col) = file_line_index.line_col(d.byte_offset);
            let (end_line, end_col) = file_line_index.line_col(d.byte_end);
            vec![Diagnostic {
                severity: Severity::Warning,
                message: "Unused graphql-analyzer-ignore directive".into(),
                range: DiagnosticRange {
                    start: Position {
                        line: start_line as u32,
                        character: start_col as u32,
                    },
                    end: Position {
                        line: end_line as u32,
                        character: end_col as u32,
                    },
                },
                source: "graphql-linter".into(),
                code: Some("unused_ignore".into()),
            }]
        }
        UnusedIgnore::UnusedRules { rules, .. } => {
            rules
                .into_iter()
                .map(|r| {
                    let (start_line, start_col) = file_line_index.line_col(r.byte_offset);
                    let (end_line, end_col) = file_line_index.line_col(r.byte_end);
                    Diagnostic {
                        severity: Severity::Warning,
                        message: format!(
                            "Unused rule '{}' in graphql-analyzer-ignore directive",
                            r.name
                        ).into(),
                        range: DiagnosticRange {
                            start: Position {
                                line: start_line as u32,
                                character: start_col as u32,
                            },
                            end: Position {
                                line: end_line as u32,
                                character: end_col as u32,
                            },
                        },
                        source: "graphql-linter".into(),
                        code: Some("unused_ignore".into()),
                    }
                })
                .collect()
        }
    })
    .collect()
```

- [ ] **Step 4: Run all ignore-related tests**

Run: `cargo test -p graphql-analysis --test analysis_tests ignore 2>&1 | tail -30`
Expected: all pass

- [ ] **Step 5: Run full linter + analysis test suite**

Run: `cargo test -p graphql-linter -p graphql-analysis 2>&1 | tail -20`
Expected: all pass

- [ ] **Step 6: Commit**

```bash
git add crates/analysis/src/lint_integration.rs crates/analysis/tests/analysis_tests.rs
git commit -m "feat(analysis): report per-rule unused ignore diagnostics

When a multi-rule ignore directive like
'# graphql-analyzer-ignore: ruleA, ruleB' only partially matches,
each unused rule gets its own diagnostic with a precise underline
on just that rule name."
```

---

### Task 3: Fix test workspace and update docs

**Files:**
- Modify: `test-workspace/lint-ignores/src/operations.graphql`
- Modify: `docs/ignoring-lint-rules.md`

- [ ] **Step 1: Fix `require_id_field` misuse in operations.graphql**

The current line 25 has `# graphql-analyzer-ignore: no_deprecated, require_id_field` above `views`. But `require_id_field` fires on the parent field whose selection set is missing `id` (i.e., `author`), not on `views`. Fix the example and add a partial-unused demonstration:

Replace the "Multiple rules in one ignore" section with a correct example where both rules actually apply to the next line, and add a new "Partial unused" section showing what happens when only some rules match.

- [ ] **Step 2: Update docs/ignoring-lint-rules.md**

Add a section explaining partial-unused behavior:
- If you list multiple rules and some don't fire, each unused rule gets its own warning
- Show example warning message: `Unused rule 'require_id_field' in graphql-analyzer-ignore directive`

- [ ] **Step 3: Commit**

```bash
git add test-workspace/lint-ignores/src/operations.graphql docs/ignoring-lint-rules.md
git commit -m "fix: correct require_id_field misuse in test workspace, document partial-unused behavior"
```

---

### Task 4: Final validation

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: all pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets 2>&1 | tail -20`
Expected: no warnings (no dead code from removed `find_unused_directives`)

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check 2>&1`
Expected: no formatting issues
