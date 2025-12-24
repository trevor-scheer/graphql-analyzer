use crate::context::StandaloneSchemaContext;
use crate::rules_old::StandaloneSchemaRule;
use apollo_compiler::schema::ExtendedType;
use graphql_project::{Diagnostic, Position, Range, Severity};

/// Lint rule that enforces input object types end with "Input"
///
/// GraphQL best practice recommends input object type names end with "Input".
/// This makes it immediately clear which types are input types vs output types.
///
/// # Examples
///
/// ```graphql
/// # ❌ Bad - input type doesn't end with "Input"
/// input CreateUser {
///   name: String!
///   email: String!
/// }
///
/// input UserFilter {
///   name: String
///   email: String
/// }
///
/// # ✅ Good - input types end with "Input"
/// input CreateUserInput {
///   name: String!
///   email: String!
/// }
///
/// input UserFilterInput {
///   name: String
///   email: String
/// }
/// ```
pub struct InputTypeSuffixRule;

impl StandaloneSchemaRule for InputTypeSuffixRule {
    fn name(&self) -> &'static str {
        "input_type_suffix"
    }

    fn description(&self) -> &'static str {
        "Require input object type names to end with 'Input'"
    }

    fn check(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let schema = ctx.schema.schema();

        for (type_name, extended_type) in &schema.types {
            if let ExtendedType::InputObject(_) = extended_type {
                if !type_name.ends_with("Input") {
                    diagnostics.push(create_diagnostic(type_name));
                }
            }
        }

        diagnostics
    }
}

/// Create a diagnostic for an input type name violation
fn create_diagnostic(type_name: &str) -> Diagnostic {
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
            "Input type '{type_name}' should end with 'Input'. Consider renaming to '{type_name}Input'."
        ),
        code: Some("input_type_suffix".to_string()),
        source: "graphql-linter".to_string(),
        related_info: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ends_with_input() {
        assert!("CreateUserInput".ends_with("Input"));
        assert!("UpdateUserInput".ends_with("Input"));
        assert!("FilterInput".ends_with("Input"));
    }

    #[test]
    fn test_does_not_end_with_input() {
        assert!(!"CreateUser".ends_with("Input"));
        assert!(!"UpdateUser".ends_with("Input"));
        assert!(!"Filter".ends_with("Input"));
    }

    #[test]
    fn test_case_sensitivity() {
        // Case matters - these should NOT match
        assert!(!"CreateUserINPUT".ends_with("Input"));
        assert!(!"UpdateUserinput".ends_with("Input"));
        assert!(!"Filterinput".ends_with("Input"));
    }

    #[test]
    fn test_input_in_middle() {
        // "Input" in the middle doesn't count
        assert!(!"UserInputData".ends_with("Input"));
        assert!(!"InputData".ends_with("Input"));
    }

    #[test]
    fn test_just_input() {
        assert!("Input".ends_with("Input"));
    }
}
