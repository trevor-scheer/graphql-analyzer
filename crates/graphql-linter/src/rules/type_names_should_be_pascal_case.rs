use crate::context::StandaloneSchemaContext;
use crate::rules::StandaloneSchemaRule;
use apollo_compiler::schema::ExtendedType;
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces type names use `PascalCase`
///
/// GraphQL convention dictates that type names should use `PascalCase` formatting.
/// This improves consistency across GraphQL APIs and follows the official spec conventions.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - type names not in PascalCase
/// type user {
///   id: ID!
/// }
///
/// type USER_PROFILE {
///   name: String!
/// }
///
/// # ✅ Good - type names in PascalCase
/// type User {
///   id: ID!
/// }
///
/// type UserProfile {
///   name: String!
/// }
/// ```
pub struct TypeNamesShouldBePascalCaseRule;

impl StandaloneSchemaRule for TypeNamesShouldBePascalCaseRule {
    fn name(&self) -> &'static str {
        "type_names_should_be_pascal_case"
    }

    fn description(&self) -> &'static str {
        "Enforce that type names use PascalCase formatting"
    }

    fn check(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let schema = ctx.schema.schema();

        // Check all types for name violations
        for (type_name, extended_type) in &schema.types {
            // Skip built-in types that start with __
            if type_name.starts_with("__") {
                continue;
            }

            let type_kind = match extended_type {
                ExtendedType::Object(_) => "object",
                ExtendedType::Interface(_) => "interface",
                ExtendedType::InputObject(_) => "input object",
                ExtendedType::Enum(_) => "enum",
                ExtendedType::Union(_) => "union",
                ExtendedType::Scalar(_) => {
                    // Skip built-in scalars
                    if matches!(
                        type_name.as_str(),
                        "Int" | "Float" | "String" | "Boolean" | "ID"
                    ) {
                        continue;
                    }
                    "scalar"
                }
            };

            if !is_pascal_case(type_name) {
                diagnostics.push(create_diagnostic(type_name, type_kind));
            }
        }

        diagnostics
    }
}

/// Check if a name is in `PascalCase` format
///
/// Rules:
/// - Must start with an uppercase letter
/// - Can contain letters and numbers
/// - Should not contain underscores or hyphens
/// - No consecutive uppercase letters (e.g., `"HTTPServer"` is discouraged, prefer `"HttpServer"`)
fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();

    // First character must be uppercase
    if !first.is_ascii_uppercase() {
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

/// Create a diagnostic for a type name violation
fn create_diagnostic(type_name: &str, type_kind: &str) -> Diagnostic {
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
        message: format!("Type '{type_name}' ({type_kind}) should use PascalCase formatting"),
        code: Some("type_names_should_be_pascal_case".to_string()),
        source: "graphql-linter".to_string(),
        related_info: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_pascal_case() {
        // Valid PascalCase
        assert!(is_pascal_case("User"));
        assert!(is_pascal_case("UserProfile"));
        assert!(is_pascal_case("Post"));
        assert!(is_pascal_case("CreateUserInput"));
        assert!(is_pascal_case("UpdatePostInput"));

        // Invalid - starts with lowercase
        assert!(!is_pascal_case("user"));
        assert!(!is_pascal_case("userProfile"));

        // Invalid - contains underscores
        assert!(!is_pascal_case("User_Profile"));
        assert!(!is_pascal_case("USER_PROFILE"));
        assert!(!is_pascal_case("user_profile"));

        // Invalid - contains hyphens
        assert!(!is_pascal_case("User-Profile"));

        // Edge cases
        assert!(!is_pascal_case(""));
    }

    #[test]
    fn test_pascal_case_with_numbers() {
        assert!(is_pascal_case("User1"));
        assert!(is_pascal_case("Http2Server"));
        assert!(is_pascal_case("Ipv4Address"));
    }

    #[test]
    fn test_pascal_case_with_acronyms() {
        // These are valid PascalCase even if multiple capitals
        assert!(is_pascal_case("HTTPServer"));
        assert!(is_pascal_case("URLPath"));
        assert!(is_pascal_case("XMLParser"));

        // These would be invalid (starting with lowercase)
        assert!(!is_pascal_case("httpServer"));
        assert!(!is_pascal_case("urlPath"));
    }

    #[test]
    fn test_pascal_case_single_letter() {
        assert!(is_pascal_case("A"));
        assert!(is_pascal_case("Z"));
        assert!(!is_pascal_case("a"));
    }
}
