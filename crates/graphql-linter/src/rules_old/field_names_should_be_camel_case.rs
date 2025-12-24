use crate::context::StandaloneSchemaContext;
use crate::rules_old::StandaloneSchemaRule;
use apollo_compiler::schema::ExtendedType;
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces field names use camelCase
///
/// GraphQL convention dictates that field names should use camelCase formatting.
/// This improves consistency across GraphQL APIs and follows the official spec conventions.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - field names not in camelCase
/// type User {
///   user_id: ID!
///   FirstName: String!
///   last_name: String!
/// }
///
/// # ✅ Good - field names in camelCase
/// type User {
///   userId: ID!
///   firstName: String!
///   lastName: String!
/// }
/// ```
pub struct FieldNamesShouldBeCamelCaseRule;

impl StandaloneSchemaRule for FieldNamesShouldBeCamelCaseRule {
    fn name(&self) -> &'static str {
        "field_names_should_be_camel_case"
    }

    fn description(&self) -> &'static str {
        "Enforce that field names use camelCase formatting"
    }

    fn check(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let schema = ctx.schema.schema();

        // Check all types for field name violations
        for (type_name, extended_type) in &schema.types {
            match extended_type {
                ExtendedType::Object(obj) => {
                    for (field_name, field) in &obj.fields {
                        if !is_camel_case(field_name) {
                            diagnostics.push(create_diagnostic(type_name, field_name, &field.name));
                        }
                    }
                }
                ExtendedType::Interface(iface) => {
                    for (field_name, field) in &iface.fields {
                        if !is_camel_case(field_name) {
                            diagnostics.push(create_diagnostic(type_name, field_name, &field.name));
                        }
                    }
                }
                ExtendedType::InputObject(input) => {
                    for (field_name, field) in &input.fields {
                        if !is_camel_case(field_name) {
                            diagnostics.push(create_diagnostic(type_name, field_name, &field.name));
                        }
                    }
                }
                _ => {} // Scalars, Enums, Unions don't have fields
            }
        }

        diagnostics
    }
}

/// Check if a name is in camelCase format
///
/// Rules:
/// - Must start with a lowercase letter
/// - Can contain letters, numbers, and internal capital letters
/// - Should not contain underscores or hyphens
/// - Leading underscores are allowed (for internal/private fields)
fn is_camel_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Allow leading underscores for internal fields
    let name = name.trim_start_matches('_');
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();

    // First character (after underscores) must be lowercase
    if !first.is_ascii_lowercase() {
        return false;
    }

    // Rest should only contain letters and numbers, no underscores or hyphens
    for ch in chars {
        if !ch.is_alphanumeric() {
            return false;
        }
    }

    true
}

/// Create a diagnostic for a field name violation
fn create_diagnostic(type_name: &str, field_name: &str, field_node_name: &str) -> Diagnostic {
    // For schema-only lints, we don't have source positions
    // Using 0,0 as placeholder - these will need to be enhanced when we have schema source maps
    let _ = field_node_name; // Unused for now, but available for future position lookup

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
            "Field '{field_name}' on type '{type_name}' should use camelCase formatting"
        ),
        code: Some("field_names_should_be_camel_case".to_string()),
        source: "graphql-linter".to_string(),
        related_info: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_camel_case() {
        // Valid camelCase
        assert!(is_camel_case("userId"));
        assert!(is_camel_case("firstName"));
        assert!(is_camel_case("isActive"));
        assert!(is_camel_case("id"));
        assert!(is_camel_case("name"));
        assert!(is_camel_case("createdAt"));
        assert!(is_camel_case("updatedAt"));

        // Leading underscores allowed
        assert!(is_camel_case("_internal"));
        assert!(is_camel_case("__typename"));

        // Invalid - starts with uppercase
        assert!(!is_camel_case("UserId"));
        assert!(!is_camel_case("FirstName"));

        // Invalid - contains underscores
        assert!(!is_camel_case("user_id"));
        assert!(!is_camel_case("first_name"));
        assert!(!is_camel_case("last_name"));

        // Invalid - contains hyphens
        assert!(!is_camel_case("user-id"));
        assert!(!is_camel_case("first-name"));

        // Edge cases
        assert!(!is_camel_case(""));
        assert!(!is_camel_case("_"));
        assert!(!is_camel_case("__"));
    }

    #[test]
    fn test_camel_case_with_numbers() {
        assert!(is_camel_case("user1"));
        assert!(is_camel_case("field2Name"));
        assert!(is_camel_case("ipv4Address"));
    }

    #[test]
    fn test_camel_case_with_acronyms() {
        // These are technically valid camelCase even if they look odd
        assert!(is_camel_case("userId"));
        assert!(is_camel_case("urlPath"));
        assert!(is_camel_case("httpStatus"));

        // These would be invalid (starting with uppercase)
        assert!(!is_camel_case("URLPath"));
        assert!(!is_camel_case("HTTPStatus"));
    }
}
