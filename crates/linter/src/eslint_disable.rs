//! Preprocess `# eslint-disable*` directive comments embedded in `.graphql`
//! source text and produce a `Suppressions` map that the lint harness can use
//! to filter out diagnostics before asserting.
//!
//! Upstream `@graphql-eslint` processes these directives at the `ESLint`
//! framework level, so rules never see the suppressed violations. We replicate
//! that behaviour as a post-processing step in our test harness rather than
//! threading suppression state through every rule's `check` signature.
//!
//! Supported directives (matching upstream):
//! - `# eslint-disable-next-line [rule, ...]` — suppresses the directive line
//!   itself and the immediately following line. Bare form (no rule list)
//!   suppresses all rules.
//! - `# eslint-disable [rule, ...]` — suppresses from this line forward until
//!   a matching `eslint-enable` (or end of file). Bare form suppresses all.
//! - `# eslint-enable [rule, ...]` — re-enables rules disabled above.

/// A resolved set of suppressed `(rule_name, line_number)` pairs derived from
/// directive comments in a single source file. Line numbers are 1-based.
pub(crate) struct Suppressions {
    /// Ranges of (`start_line`, `end_line`, `rule_or_none`) that are suppressed.
    /// `rule` is `None` for bare directives that suppress every rule.
    ranges: Vec<SuppressionRange>,
}

struct SuppressionRange {
    /// First 1-based line that is suppressed (inclusive).
    start_line: u32,
    /// Last 1-based line that is suppressed (inclusive).
    end_line: u32,
    /// The specific rule suppressed, or `None` for "all rules".
    rule: Option<String>,
}

impl Suppressions {
    /// Parse `source` and build a `Suppressions` map.
    pub(crate) fn from_source(source: &str) -> Self {
        let mut ranges = Vec::new();

        // Track open `eslint-disable` blocks: rule_or_none → start_line.
        let mut open_blocks: Vec<(Option<String>, u32)> = Vec::new();

        for (zero_idx, line) in source.lines().enumerate() {
            let line_no = zero_idx as u32 + 1;
            let trimmed = line.trim();

            if let Some(directive) = parse_eslint_directive(trimmed) {
                match directive.kind {
                    DirectiveKind::DisableNextLine => {
                        // The directive comment itself is on `line_no`.
                        // The "next" target line is `line_no + 1`.
                        // We suppress both so that a rule firing ON the
                        // directive comment (e.g. no-hashtag-description) is
                        // also covered.
                        let end = line_no + 1;
                        if directive.rules.is_empty() {
                            ranges.push(SuppressionRange {
                                start_line: line_no,
                                end_line: end,
                                rule: None,
                            });
                        } else {
                            for rule in directive.rules {
                                ranges.push(SuppressionRange {
                                    start_line: line_no,
                                    end_line: end,
                                    rule: Some(rule),
                                });
                            }
                        }
                    }
                    DirectiveKind::Disable => {
                        if directive.rules.is_empty() {
                            open_blocks.push((None, line_no));
                        } else {
                            for rule in directive.rules {
                                open_blocks.push((Some(rule), line_no));
                            }
                        }
                    }
                    DirectiveKind::Enable => {
                        if directive.rules.is_empty() {
                            // Re-enable everything: close all open blocks.
                            for (rule_opt, start) in open_blocks.drain(..) {
                                ranges.push(SuppressionRange {
                                    start_line: start,
                                    end_line: line_no,
                                    rule: rule_opt,
                                });
                            }
                        } else {
                            for rule in directive.rules {
                                // Close the most-recent open block for this rule.
                                if let Some(pos) = open_blocks
                                    .iter()
                                    .rposition(|(r, _)| r.as_deref() == Some(&rule))
                                {
                                    let (rule_opt, start) = open_blocks.remove(pos);
                                    ranges.push(SuppressionRange {
                                        start_line: start,
                                        end_line: line_no,
                                        rule: rule_opt,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Any unclosed `eslint-disable` blocks extend to the end of the file.
        // Use u32::MAX as a sentinel meaning "no upper bound".
        for (rule_opt, start) in open_blocks {
            ranges.push(SuppressionRange {
                start_line: start,
                end_line: u32::MAX,
                rule: rule_opt,
            });
        }

        Self { ranges }
    }

    /// Returns `true` if `rule_name` at `line` (1-based) is suppressed.
    pub(crate) fn is_suppressed(&self, rule_name: &str, line: u32) -> bool {
        self.ranges.iter().any(|r| {
            line >= r.start_line
                && line <= r.end_line
                && (r.rule.is_none() || r.rule.as_deref() == Some(rule_name))
        })
    }
}

#[derive(Debug)]
enum DirectiveKind {
    DisableNextLine,
    Disable,
    Enable,
}

struct ParsedDirective {
    kind: DirectiveKind,
    /// Rule names parsed from the comma-separated list. Empty = bare (all rules).
    rules: Vec<String>,
}

/// Try to parse a trimmed line as a `# eslint-disable*` directive.
/// Returns `None` if the line is not a recognised directive.
fn parse_eslint_directive(trimmed: &str) -> Option<ParsedDirective> {
    // Must start with `#`; strip it and any leading whitespace.
    let after_hash = trimmed.strip_prefix('#')?.trim_start();

    let (kind, rest) = if let Some(r) = after_hash.strip_prefix("eslint-disable-next-line") {
        (DirectiveKind::DisableNextLine, r)
    } else if let Some(r) = after_hash.strip_prefix("eslint-disable") {
        (DirectiveKind::Disable, r)
    } else if let Some(r) = after_hash.strip_prefix("eslint-enable") {
        (DirectiveKind::Enable, r)
    } else {
        return None;
    };

    // After the keyword there should be nothing, whitespace only, or a
    // whitespace-then-comma-separated rule list.
    let rules_str = rest.trim();
    let rules: Vec<String> = if rules_str.is_empty() {
        Vec::new()
    } else {
        rules_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    Some(ParsedDirective { kind, rules })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_disable_next_line_suppresses_all_rules() {
        let src = "# eslint-disable-next-line\ntype Query {\n  foo: String\n}\n";
        let s = Suppressions::from_source(src);
        // The directive is on line 1; next line is 2.
        assert!(s.is_suppressed("no-hashtag-description", 1));
        assert!(s.is_suppressed("no-hashtag-description", 2));
        assert!(s.is_suppressed("anything", 2));
        assert!(!s.is_suppressed("anything", 3));
    }

    #[test]
    fn named_disable_next_line_suppresses_only_named_rule() {
        let src = "type User {\n  # eslint-disable-next-line noTypenamePrefix\n  userId: ID!\n}";
        let s = Suppressions::from_source(src);
        // directive is on line 2, next is line 3
        assert!(s.is_suppressed("noTypenamePrefix", 3));
        assert!(!s.is_suppressed("otherRule", 3));
    }

    #[test]
    fn disable_enable_block() {
        let src = "# eslint-disable myRule\ntype A {}\n# eslint-enable myRule\ntype B {}\n";
        let s = Suppressions::from_source(src);
        assert!(s.is_suppressed("myRule", 2));
        assert!(!s.is_suppressed("myRule", 4));
    }

    #[test]
    fn unclosed_disable_block_extends_to_eof() {
        let src = "# eslint-disable myRule\ntype A {}\n";
        let s = Suppressions::from_source(src);
        assert!(s.is_suppressed("myRule", 2));
        assert!(s.is_suppressed("myRule", 99));
    }
}
