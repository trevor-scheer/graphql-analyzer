//! SDL (Schema Definition Language) conversion from introspection responses.

use crate::types::{IntrospectionField, IntrospectionResponse, IntrospectionType};
use std::fmt::Write;

/// Built-in GraphQL scalar types that should not be included in generated SDL.
const BUILTIN_SCALARS: &[&str] = &["Int", "Float", "String", "Boolean", "ID"];

/// Converts a GraphQL introspection response to SDL (Schema Definition Language).
///
/// This function generates clean, readable SDL from an introspection response by:
/// - Filtering out built-in scalar types (Int, Float, String, Boolean, ID)
/// - Filtering out introspection types (types starting with `__`)
/// - Filtering out built-in directives (@skip, @include, @deprecated, @specifiedBy)
/// - Preserving descriptions, deprecation information, and custom directives
/// - Formatting with proper indentation and GraphQL syntax
///
/// # Arguments
///
/// * `introspection` - The introspection response to convert
///
/// # Returns
///
/// Returns a formatted SDL string representing the schema.
///
/// # Examples
///
/// ```no_run
/// # use graphql_introspect::{execute_introspection, introspection_to_sdl};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let introspection = execute_introspection("https://api.example.com/graphql").await?;
/// let sdl = introspection_to_sdl(&introspection);
/// println!("{}", sdl);
/// # Ok(())
/// # }
/// ```
#[must_use]
#[tracing::instrument(skip(introspection), fields(
    types = introspection.data.schema.types.len(),
    directives = introspection.data.schema.directives.len()
))]
pub fn introspection_to_sdl(introspection: &IntrospectionResponse) -> String {
    tracing::debug!("Converting introspection to SDL");
    let mut sdl = String::new();
    let schema = &introspection.data.schema;

    let needs_schema_def = schema
        .query_type
        .as_ref()
        .is_some_and(|t| t.name != "Query")
        || schema
            .mutation_type
            .as_ref()
            .is_some_and(|t| t.name != "Mutation")
        || schema
            .subscription_type
            .as_ref()
            .is_some_and(|t| t.name != "Subscription");

    if needs_schema_def {
        sdl.push_str("schema {\n");
        if let Some(ref query) = schema.query_type {
            writeln!(sdl, "  query: {}", query.name).unwrap();
        }
        if let Some(ref mutation) = schema.mutation_type {
            writeln!(sdl, "  mutation: {}", mutation.name).unwrap();
        }
        if let Some(ref subscription) = schema.subscription_type {
            writeln!(sdl, "  subscription: {}", subscription.name).unwrap();
        }
        sdl.push_str("}\n\n");
    }

    for directive in &schema.directives {
        if directive.name == "skip"
            || directive.name == "include"
            || directive.name == "deprecated"
            || directive.name == "specifiedBy"
        {
            continue;
        }

        write_description(&mut sdl, directive.description.as_ref(), 0);
        write!(sdl, "directive @{}", directive.name).unwrap();

        if !directive.args.is_empty() {
            sdl.push('(');
            for (i, arg) in directive.args.iter().enumerate() {
                if i > 0 {
                    sdl.push_str(", ");
                }
                write!(sdl, "{}: {}", arg.name, arg.type_ref.to_type_string()).unwrap();
                if let Some(default) = &arg.default_value {
                    write!(sdl, " = {default}").unwrap();
                }
            }
            sdl.push(')');
        }

        sdl.push_str(" on ");
        sdl.push_str(&directive.locations.join(" | "));
        sdl.push_str("\n\n");
    }

    let mut types_written = 0;
    for type_def in &schema.types {
        let name = type_name(type_def);
        if name.starts_with("__") || BUILTIN_SCALARS.contains(&name) {
            continue;
        }

        write_type(&mut sdl, type_def);
        sdl.push_str("\n\n");
        types_written += 1;
    }

    tracing::debug!(
        types_written,
        sdl_length = sdl.len(),
        "SDL generation complete"
    );
    sdl.trim_end().to_string()
}

fn type_name(type_def: &IntrospectionType) -> &str {
    match type_def {
        IntrospectionType::Scalar(t) => &t.name,
        IntrospectionType::Object(t) => &t.name,
        IntrospectionType::Interface(t) => &t.name,
        IntrospectionType::Union(t) => &t.name,
        IntrospectionType::Enum(t) => &t.name,
        IntrospectionType::InputObject(t) => &t.name,
    }
}

fn write_type(sdl: &mut String, type_def: &IntrospectionType) {
    match type_def {
        IntrospectionType::Scalar(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            writeln!(sdl, "scalar {}", t.name).unwrap();
        }
        IntrospectionType::Object(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            write!(sdl, "type {}", t.name).unwrap();

            if !t.interfaces.is_empty() {
                sdl.push_str(" implements ");
                for (i, interface) in t.interfaces.iter().enumerate() {
                    if i > 0 {
                        sdl.push_str(" & ");
                    }
                    sdl.push_str(&interface.name);
                }
            }

            if t.fields.is_empty() {
                sdl.push_str(" {\n}");
            } else {
                sdl.push_str(" {\n");
                for field in &t.fields {
                    write_field(sdl, field, 1);
                }
                sdl.push('}');
            }
        }
        IntrospectionType::Interface(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            write!(sdl, "interface {}", t.name).unwrap();

            if !t.interfaces.is_empty() {
                sdl.push_str(" implements ");
                for (i, interface) in t.interfaces.iter().enumerate() {
                    if i > 0 {
                        sdl.push_str(" & ");
                    }
                    sdl.push_str(&interface.name);
                }
            }

            if t.fields.is_empty() {
                sdl.push_str(" {\n}");
            } else {
                sdl.push_str(" {\n");
                for field in &t.fields {
                    write_field(sdl, field, 1);
                }
                sdl.push('}');
            }
        }
        IntrospectionType::Union(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            write!(sdl, "union {} = ", t.name).unwrap();
            for (i, member) in t.possible_types.iter().enumerate() {
                if i > 0 {
                    sdl.push_str(" | ");
                }
                sdl.push_str(&member.name);
            }
        }
        IntrospectionType::Enum(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            writeln!(sdl, "enum {} {{", t.name).unwrap();
            for value in &t.enum_values {
                write_description(sdl, value.description.as_ref(), 1);
                write!(sdl, "  {}", value.name).unwrap();
                if value.is_deprecated {
                    if let Some(ref reason) = value.deprecation_reason {
                        write!(sdl, " @deprecated(reason: \"{}\")", escape_string(reason)).unwrap();
                    } else {
                        sdl.push_str(" @deprecated");
                    }
                }
                sdl.push('\n');
            }
            sdl.push('}');
        }
        IntrospectionType::InputObject(t) => {
            write_description(sdl, t.description.as_ref(), 0);
            writeln!(sdl, "input {} {{", t.name).unwrap();
            for field in &t.input_fields {
                write_description(sdl, field.description.as_ref(), 1);
                write!(sdl, "  {}: {}", field.name, field.type_ref.to_type_string()).unwrap();
                if let Some(default) = &field.default_value {
                    write!(sdl, " = {default}").unwrap();
                }
                sdl.push('\n');
            }
            sdl.push('}');
        }
    }
}

fn write_field(sdl: &mut String, field: &IntrospectionField, indent: usize) {
    let indent_str = "  ".repeat(indent);

    write_description(sdl, field.description.as_ref(), indent);
    write!(sdl, "{indent_str}{}", field.name).unwrap();

    if !field.args.is_empty() {
        sdl.push('(');
        for (i, arg) in field.args.iter().enumerate() {
            if i > 0 {
                sdl.push_str(", ");
            }
            write!(sdl, "{}: {}", arg.name, arg.type_ref.to_type_string()).unwrap();
            if let Some(default) = &arg.default_value {
                write!(sdl, " = {default}").unwrap();
            }
        }
        sdl.push(')');
    }

    write!(sdl, ": {}", field.type_ref.to_type_string()).unwrap();

    if field.is_deprecated {
        if let Some(ref reason) = field.deprecation_reason {
            write!(sdl, " @deprecated(reason: \"{}\")", escape_string(reason)).unwrap();
        } else {
            sdl.push_str(" @deprecated");
        }
    }

    sdl.push('\n');
}

fn write_description(sdl: &mut String, description: Option<&String>, indent: usize) {
    if let Some(desc) = description {
        let indent_str = "  ".repeat(indent);
        if desc.contains('\n') {
            writeln!(sdl, "{indent_str}\"\"\"\n{desc}\n{indent_str}\"\"\"").unwrap();
        } else {
            writeln!(sdl, "{indent_str}\"{}\"", escape_string(desc)).unwrap();
        }
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IntrospectionTypeRefFull, TypeKind};

    #[test]
    fn test_type_ref_to_string() {
        let type_ref = IntrospectionTypeRefFull {
            kind: TypeKind::NonNull,
            name: None,
            of_type: Some(Box::new(IntrospectionTypeRefFull {
                kind: TypeKind::Scalar,
                name: Some("String".to_string()),
                of_type: None,
            })),
        };
        assert_eq!(type_ref.to_type_string(), "String!");

        let type_ref = IntrospectionTypeRefFull {
            kind: TypeKind::List,
            name: None,
            of_type: Some(Box::new(IntrospectionTypeRefFull {
                kind: TypeKind::Scalar,
                name: Some("String".to_string()),
                of_type: None,
            })),
        };
        assert_eq!(type_ref.to_type_string(), "[String]");

        let type_ref = IntrospectionTypeRefFull {
            kind: TypeKind::NonNull,
            name: None,
            of_type: Some(Box::new(IntrospectionTypeRefFull {
                kind: TypeKind::List,
                name: None,
                of_type: Some(Box::new(IntrospectionTypeRefFull {
                    kind: TypeKind::Scalar,
                    name: Some("String".to_string()),
                    of_type: None,
                })),
            })),
        };
        assert_eq!(type_ref.to_type_string(), "[String]!");
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_string("hello\nworld"), "hello\\nworld");
        assert_eq!(
            escape_string("C:\\path\\to\\file"),
            "C:\\\\path\\\\to\\\\file"
        );
    }
}
