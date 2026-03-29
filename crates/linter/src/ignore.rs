//! Support for inline lint ignore comments.
//!
//! Users can suppress lint diagnostics on a per-case basis using comments:
//!
//! ```graphql
//! # graphql-analyzer-ignore
//! query { ... }
//!
//! # graphql-analyzer-ignore: noDeprecated, unusedVariables
//! query { ... }
//! ```
//!
//! The comment must appear on the line immediately before the diagnostic.
//! Without rule names, all lint rules are suppressed for that line.

/// Byte range of a single rule name within an ignore comment.
/// Byte offsets are file-relative (not line-relative).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpan {
    pub name: String,
    pub byte_offset: usize,
    pub byte_end: usize,
}

/// A parsed ignore directive from a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreDirective {
    /// The line number this directive appears on (0-based).
    pub line: usize,
    /// Byte offset of the start of this comment line in the source.
    pub byte_offset: usize,
    /// Byte offset of the end of this comment line in the source.
    pub byte_end: usize,
    /// The rules to ignore. Empty means ignore all rules.
    pub rules: Vec<RuleSpan>,
}

impl IgnoreDirective {
    /// Returns true if this directive suppresses the given rule.
    #[must_use]
    pub fn suppresses(&self, rule_name: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|r| r.name == rule_name)
    }

    #[must_use]
    pub fn rule_names(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.name.as_str()).collect()
    }
}

const IGNORE_PREFIX: &str = "graphql-analyzer-ignore";

/// Parse all ignore directives from GraphQL source text.
///
/// Scans each line for comments matching `# graphql-analyzer-ignore`
/// or `# graphql-analyzer-ignore: rule1, rule2`.
#[must_use]
pub fn parse_ignore_directives(source: &str) -> Vec<IgnoreDirective> {
    let mut directives = Vec::new();
    let mut byte_pos = 0;

    for (line_num, line) in source.lines().enumerate() {
        let line_start = byte_pos;
        let line_end = byte_pos + line.len();
        // Advance past line content + newline character
        byte_pos = line_end + usize::from(source[line_end..].starts_with('\n'));

        let trimmed = line.trim();

        // GraphQL comments start with #
        let Some(comment_body) = trimmed.strip_prefix('#') else {
            continue;
        };

        let comment_body = comment_body.trim();

        let Some(rest) = comment_body.strip_prefix(IGNORE_PREFIX) else {
            continue;
        };

        let rest = rest.trim();

        if rest.is_empty() {
            directives.push(IgnoreDirective {
                line: line_num,
                byte_offset: line_start,
                byte_end: line_end,
                rules: Vec::new(),
            });
        } else if let Some(rule_list) = rest.strip_prefix(':') {
            // rule_list is a sub-slice of line (via trim -> strip_prefix chain),
            // so pointer arithmetic gives us its exact position within line.
            let rule_list_file_offset =
                line_start + (rule_list.as_ptr() as usize - line.as_ptr() as usize);

            let mut rules = Vec::new();
            let mut pos = 0;
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
    }

    directives
}

/// Check if a diagnostic at the given line should be suppressed.
///
/// A diagnostic is suppressed if there is an ignore directive on the
/// immediately preceding line that covers the diagnostic's rule.
#[must_use]
pub fn is_suppressed(directives: &[IgnoreDirective], diagnostic_line: usize, rule: &str) -> bool {
    if diagnostic_line == 0 {
        return false;
    }

    let preceding_line = diagnostic_line - 1;
    directives
        .iter()
        .any(|d| d.line == preceding_line && d.suppresses(rule))
}

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

/// Find ignore directives where individual rules didn't suppress any diagnostic.
///
/// Provides per-rule granularity: if a multi-rule directive has some rules that
/// matched and some that didn't, only the unmatched rules are reported.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_ignore() {
        let source = "# graphql-analyzer-ignore\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].line, 0);
        assert!(directives[0].rules.is_empty());
        assert!(directives[0].suppresses("any_rule"));
    }

    #[test]
    fn parse_ignore_with_rules() {
        let source = "# graphql-analyzer-ignore: noDeprecated, unusedVariables\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rule_names(),
            vec!["noDeprecated", "unusedVariables"]
        );
        assert!(directives[0].suppresses("noDeprecated"));
        assert!(directives[0].suppresses("unusedVariables"));
        assert!(!directives[0].suppresses("other_rule"));
    }

    #[test]
    fn parse_ignore_with_extra_whitespace() {
        let source =
            "  #  graphql-analyzer-ignore :  noDeprecated ,  unusedVariables  \nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rule_names(),
            vec!["noDeprecated", "unusedVariables"]
        );
        assert_eq!(
            &source[directives[0].rules[0].byte_offset..directives[0].rules[0].byte_end],
            "noDeprecated"
        );
        assert_eq!(
            &source[directives[0].rules[1].byte_offset..directives[0].rules[1].byte_end],
            "unusedVariables"
        );
    }

    #[test]
    fn no_directives_in_regular_comments() {
        let source = "# This is a regular comment\n# Another comment\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert!(directives.is_empty());
    }

    #[test]
    fn malformed_directive_ignored() {
        let source = "# graphql-analyzer-ignorefoo\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert!(directives.is_empty());
    }

    #[test]
    fn multiple_directives() {
        let source = "\
# graphql-analyzer-ignore: noDeprecated
query Foo { hello }
# graphql-analyzer-ignore
query Bar { world }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].line, 0);
        assert_eq!(directives[1].line, 2);
    }

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

    #[test]
    fn is_suppressed_on_preceding_line() {
        let directives = vec![directive(2, vec!["noDeprecated"])];
        assert!(is_suppressed(&directives, 3, "noDeprecated"));
        assert!(!is_suppressed(&directives, 3, "other_rule"));
        assert!(!is_suppressed(&directives, 4, "noDeprecated"));
    }

    #[test]
    fn is_suppressed_bare_ignore() {
        let directives = vec![directive(0, vec![])];
        assert!(is_suppressed(&directives, 1, "any_rule"));
        assert!(is_suppressed(&directives, 1, "another_rule"));
    }

    #[test]
    fn is_suppressed_line_zero() {
        let directives = vec![directive(0, vec![])];
        // Can't suppress line 0 since there's no preceding line
        assert!(!is_suppressed(&directives, 0, "any_rule"));
    }

    #[test]
    fn single_rule_ignore() {
        let source = "# graphql-analyzer-ignore: noAnonymousOperations\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rule_names(), vec!["noAnonymousOperations"]);
    }

    #[test]
    fn byte_offsets_are_correct() {
        let source = "query A { a }\n# graphql-analyzer-ignore\nquery B { b }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].byte_offset, 14); // after "query A { a }\n"
        assert_eq!(directives[0].byte_end, 39); // end of "# graphql-analyzer-ignore"
        assert_eq!(&source[14..39], "# graphql-analyzer-ignore");
    }

    #[test]
    fn find_unused_all_used() {
        let directives = vec![directive(0, vec!["noDeprecated"])];
        let diag_lines = vec![(1, "noDeprecated")];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert!(unused.is_empty());
    }

    #[test]
    fn find_unused_bare_ignore_used() {
        let directives = vec![directive(0, vec![])];
        let diag_lines = vec![(1, "any_rule")];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert!(unused.is_empty());
    }

    #[test]
    fn find_unused_none_matched() {
        let directives = vec![directive(0, vec!["noDeprecated"])];
        let diag_lines: Vec<(usize, &str)> = vec![];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::EntireDirective(d) => assert_eq!(d.line, 0),
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective, got {other:?}")
            }
        }
    }

    #[test]
    fn find_unused_wrong_rule() {
        let directives = vec![directive(0, vec!["noDeprecated"])];
        let diag_lines = vec![(1, "other_rule")];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::EntireDirective(d) => assert_eq!(d.line, 0),
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective, got {other:?}")
            }
        }
    }

    #[test]
    fn find_unused_wrong_line() {
        let directives = vec![directive(0, vec!["noDeprecated"])];
        let diag_lines = vec![(5, "noDeprecated")];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::EntireDirective(d) => assert_eq!(d.line, 0),
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective, got {other:?}")
            }
        }
    }

    #[test]
    fn find_unused_rules_partial() {
        let directives = vec![directive(0, vec!["noDeprecated", "requireIdField"])];
        let diag_lines = vec![(1, "noDeprecated")];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::UnusedRules { directive, rules } => {
                assert_eq!(directive.line, 0);
                let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
                assert_eq!(names, vec!["requireIdField"]);
            }
            other @ UnusedIgnore::EntireDirective(_) => {
                panic!("Expected UnusedRules, got {other:?}")
            }
        }
    }

    #[test]
    fn find_unused_rules_all_rules_unused() {
        let directives = vec![directive(0, vec!["noDeprecated", "requireIdField"])];
        let diag_lines: Vec<(usize, &str)> = vec![];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::EntireDirective(d) => assert_eq!(d.line, 0),
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective, got {other:?}")
            }
        }
    }

    #[test]
    fn find_unused_rules_all_used() {
        let directives = vec![directive(0, vec!["noDeprecated", "requireIdField"])];
        let diag_lines = vec![(1, "noDeprecated"), (1, "requireIdField")];
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
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective for bare ignore, got {other:?}")
            }
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
        let directives = vec![directive(0, vec!["noDeprecated"])];
        let diag_lines: Vec<(usize, &str)> = vec![];
        let unused = find_unused_rules(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        match &unused[0] {
            UnusedIgnore::EntireDirective(d) => {
                let names: Vec<&str> = d.rule_names();
                assert_eq!(names, vec!["noDeprecated"]);
            }
            other @ UnusedIgnore::UnusedRules { .. } => {
                panic!("Expected EntireDirective for single unused rule, got {other:?}")
            }
        }
    }

    #[test]
    fn rule_span_byte_offsets_are_correct() {
        let source = "# graphql-analyzer-ignore: noDeprecated, requireIdField\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        let rules = &directives[0].rules;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "noDeprecated");
        assert_eq!(
            &source[rules[0].byte_offset..rules[0].byte_end],
            "noDeprecated"
        );
        assert_eq!(rules[1].name, "requireIdField");
        assert_eq!(
            &source[rules[1].byte_offset..rules[1].byte_end],
            "requireIdField"
        );
    }
}
