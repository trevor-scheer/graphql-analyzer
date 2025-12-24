use crate::context::StandaloneSchemaContext;
use crate::rules_old::StandaloneSchemaRule;
use apollo_compiler::schema::ExtendedType;
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces enum values use `SCREAMING_SNAKE_CASE`
///
/// GraphQL convention dictates that enum values should use `SCREAMING_SNAKE_CASE` formatting.
/// This improves consistency across GraphQL APIs and follows the official spec conventions.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - enum values not in SCREAMING_SNAKE_CASE
/// enum Status {
///   active
///   Pending
///   in-progress
///   completedSuccessfully
/// }
///
/// # ✅ Good - enum values in SCREAMING_SNAKE_CASE
/// enum Status {
///   ACTIVE
///   PENDING
///   IN_PROGRESS
///   COMPLETED_SUCCESSFULLY
/// }
/// ```
pub struct EnumValuesShouldBeScreamingSnakeCaseRule;

impl StandaloneSchemaRule for EnumValuesShouldBeScreamingSnakeCaseRule {
    fn name(&self) -> &'static str {
        "enum_values_should_be_screaming_snake_case"
    }

    fn description(&self) -> &'static str {
        "Enforce that enum values use SCREAMING_SNAKE_CASE formatting"
    }

    fn check(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let schema = ctx.schema.schema();

        // Check all enum types for value violations
        for (type_name, extended_type) in &schema.types {
            if let ExtendedType::Enum(enum_type) = extended_type {
                for (value_name, _value) in &enum_type.values {
                    if !is_screaming_snake_case(value_name) {
                        diagnostics.push(create_diagnostic(type_name, value_name));
                    }
                }
            }
        }

        diagnostics
    }
}

/// Check if a name is in `SCREAMING_SNAKE_CASE` format
///
/// Rules:
/// - All characters must be uppercase letters, numbers, or underscores
/// - Must not start or end with an underscore
/// - Must not have consecutive underscores
/// - Must contain at least one letter
fn is_screaming_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must not start or end with underscore
    if name.starts_with('_') || name.ends_with('_') {
        return false;
    }

    // Must contain at least one letter
    if !name.chars().any(char::is_alphabetic) {
        return false;
    }

    let mut prev_was_underscore = false;

    for ch in name.chars() {
        match ch {
            'A'..='Z' | '0'..='9' => {
                prev_was_underscore = false;
            }
            '_' => {
                // No consecutive underscores
                if prev_was_underscore {
                    return false;
                }
                prev_was_underscore = true;
            }
            _ => {
                // No lowercase letters, hyphens, or other characters
                return false;
            }
        }
    }

    true
}

/// Create a diagnostic for an enum value violation
fn create_diagnostic(type_name: &str, value_name: &str) -> Diagnostic {
    // For schema-only lints, we don't have source positions
    // Using 0,0 as placeholder - these will need to be enhanced when we have schema source maps
    Diagnostic {
        severity: Severity::Warning,
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        },
        message: format!(
            "Enum value '{value_name}' on enum '{type_name}' should use SCREAMING_SNAKE_CASE formatting"
        ),
        code: Some("enum_values_should_be_screaming_snake_case".to_string()),
        source: "graphql-linter".to_string(),
        related_info: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_screaming_snake_case() {
        // Valid SCREAMING_SNAKE_CASE
        assert!(is_screaming_snake_case("ACTIVE"));
        assert!(is_screaming_snake_case("PENDING"));
        assert!(is_screaming_snake_case("IN_PROGRESS"));
        assert!(is_screaming_snake_case("COMPLETED_SUCCESSFULLY"));
        assert!(is_screaming_snake_case("STATUS_1"));
        assert!(is_screaming_snake_case("HTTP_200_OK"));

        // Invalid - lowercase letters
        assert!(!is_screaming_snake_case("active"));
        assert!(!is_screaming_snake_case("Active"));
        assert!(!is_screaming_snake_case("ACTIVE_pending"));
        assert!(!is_screaming_snake_case("in_progress"));

        // Invalid - starts or ends with underscore
        assert!(!is_screaming_snake_case("_ACTIVE"));
        assert!(!is_screaming_snake_case("ACTIVE_"));
        assert!(!is_screaming_snake_case("_ACTIVE_"));

        // Invalid - consecutive underscores
        assert!(!is_screaming_snake_case("ACTIVE__PENDING"));
        assert!(!is_screaming_snake_case("IN__PROGRESS"));

        // Invalid - hyphens
        assert!(!is_screaming_snake_case("IN-PROGRESS"));
        assert!(!is_screaming_snake_case("ACTIVE-PENDING"));

        // Invalid - only numbers or underscores (no letters)
        assert!(!is_screaming_snake_case("123"));
        assert!(!is_screaming_snake_case("_"));

        // Edge cases
        assert!(!is_screaming_snake_case(""));
    }

    #[test]
    fn test_screaming_snake_case_with_numbers() {
        assert!(is_screaming_snake_case("STATUS_1"));
        assert!(is_screaming_snake_case("HTTP_200"));
        assert!(is_screaming_snake_case("IPV4_ADDRESS"));
        assert!(is_screaming_snake_case("ERROR_404_NOT_FOUND"));
    }

    #[test]
    fn test_screaming_snake_case_single_word() {
        assert!(is_screaming_snake_case("ACTIVE"));
        assert!(is_screaming_snake_case("PENDING"));
        assert!(is_screaming_snake_case("A"));
        assert!(!is_screaming_snake_case("a"));
    }

    #[test]
    fn test_screaming_snake_case_camel_case() {
        // Common mistakes - camelCase or PascalCase should be rejected
        assert!(!is_screaming_snake_case("ActiveStatus"));
        assert!(!is_screaming_snake_case("activeStatus"));
        assert!(!is_screaming_snake_case("inProgress"));
    }
}
