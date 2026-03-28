//! Support for inline lint ignore comments.
//!
//! Users can suppress lint diagnostics on a per-case basis using comments:
//!
//! ```graphql
//! # graphql-analyzer-ignore
//! query { ... }
//!
//! # graphql-analyzer-ignore: no_deprecated, unused_variables
//! query { ... }
//! ```
//!
//! The comment must appear on the line immediately before the diagnostic.
//! Without rule names, all lint rules are suppressed for that line.

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
    pub rules: Vec<String>,
}

impl IgnoreDirective {
    /// Returns true if this directive suppresses the given rule.
    #[must_use]
    pub fn suppresses(&self, rule_name: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|r| r == rule_name)
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
            let rules = rule_list
                .split(',')
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect();
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

/// Find ignore directives that didn't suppress any diagnostic.
///
/// Takes the directives and the lines/rules of all diagnostics that were
/// produced (before filtering). Returns directives that matched nothing.
#[must_use]
pub fn find_unused_directives<'a>(
    directives: &'a [IgnoreDirective],
    diagnostic_lines_and_rules: &[(usize, &str)],
) -> Vec<&'a IgnoreDirective> {
    directives
        .iter()
        .filter(|d| {
            let target_line = d.line + 1;
            !diagnostic_lines_and_rules
                .iter()
                .any(|(line, rule)| *line == target_line && d.suppresses(rule))
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
        let source = "# graphql-analyzer-ignore: no_deprecated, unused_variables\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rules,
            vec!["no_deprecated", "unused_variables"]
        );
        assert!(directives[0].suppresses("no_deprecated"));
        assert!(directives[0].suppresses("unused_variables"));
        assert!(!directives[0].suppresses("other_rule"));
    }

    #[test]
    fn parse_ignore_with_extra_whitespace() {
        let source =
            "  #  graphql-analyzer-ignore :  no_deprecated ,  unused_variables  \nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(
            directives[0].rules,
            vec!["no_deprecated", "unused_variables"]
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
# graphql-analyzer-ignore: no_deprecated
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
            rules: rules.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn is_suppressed_on_preceding_line() {
        let directives = vec![directive(2, vec!["no_deprecated"])];
        assert!(is_suppressed(&directives, 3, "no_deprecated"));
        assert!(!is_suppressed(&directives, 3, "other_rule"));
        assert!(!is_suppressed(&directives, 4, "no_deprecated"));
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
        let source = "# graphql-analyzer-ignore: no_anonymous_operations\nquery { hello }";
        let directives = parse_ignore_directives(source);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["no_anonymous_operations"]);
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
        let directives = vec![directive(0, vec!["no_deprecated"])];
        let diag_lines = vec![(1, "no_deprecated")];
        let unused = find_unused_directives(&directives, &diag_lines);
        assert!(unused.is_empty());
    }

    #[test]
    fn find_unused_bare_ignore_used() {
        let directives = vec![directive(0, vec![])];
        let diag_lines = vec![(1, "any_rule")];
        let unused = find_unused_directives(&directives, &diag_lines);
        assert!(unused.is_empty());
    }

    #[test]
    fn find_unused_none_matched() {
        let directives = vec![directive(0, vec!["no_deprecated"])];
        let diag_lines: Vec<(usize, &str)> = vec![];
        let unused = find_unused_directives(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].line, 0);
    }

    #[test]
    fn find_unused_wrong_rule() {
        let directives = vec![directive(0, vec!["no_deprecated"])];
        let diag_lines = vec![(1, "other_rule")];
        let unused = find_unused_directives(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
    }

    #[test]
    fn find_unused_wrong_line() {
        let directives = vec![directive(0, vec!["no_deprecated"])];
        let diag_lines = vec![(5, "no_deprecated")];
        let unused = find_unused_directives(&directives, &diag_lines);
        assert_eq!(unused.len(), 1);
    }
}
