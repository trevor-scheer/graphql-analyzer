//! SDL printer for reconstructing schema SDL from the merged HIR types.
//!
//! This generates valid GraphQL SDL from the resolved TypeDefMap, giving
//! agents a single canonical view of the schema with all extensions merged.

use graphql_hir::{TypeDef, TypeDefKind, TypeDefMap};

/// Built-in scalars that should not be printed
const BUILTIN_SCALARS: &[&str] = &["String", "Int", "Float", "Boolean", "ID"];

/// Print the full schema SDL from a merged TypeDefMap.
pub fn print_schema_sdl(types: &TypeDefMap) -> String {
    let mut output = String::new();
    let mut sorted_types: Vec<_> = types.iter().collect();

    // Sort: root types first (Query, Mutation, Subscription), then alphabetically
    sorted_types.sort_by(|(name_a, _), (name_b, _)| {
        fn root_order(name: &str) -> u8 {
            match name {
                "Query" => 0,
                "Mutation" => 1,
                "Subscription" => 2,
                _ => 3,
            }
        }
        let ord_a = root_order(name_a);
        let ord_b = root_order(name_b);
        ord_a.cmp(&ord_b).then_with(|| name_a.cmp(name_b))
    });

    let mut first = true;
    for (_, type_def) in &sorted_types {
        // Skip built-in scalars
        if type_def.kind == TypeDefKind::Scalar && BUILTIN_SCALARS.contains(&type_def.name.as_ref())
        {
            continue;
        }

        if !first {
            output.push('\n');
        }
        first = false;

        print_type_def(&mut output, type_def);
    }

    output
}

fn print_type_def(out: &mut String, td: &TypeDef) {
    print_description(out, td.description.as_deref(), "");

    match td.kind {
        TypeDefKind::Scalar => print_scalar(out, td),
        TypeDefKind::Enum => print_enum(out, td),
        TypeDefKind::Union => print_union(out, td),
        TypeDefKind::Object => print_object(out, td, "type"),
        TypeDefKind::Interface => print_object(out, td, "interface"),
        TypeDefKind::InputObject => print_input_object(out, td),
        _ => {}
    }
}

fn print_description(out: &mut String, desc: Option<&str>, indent: &str) {
    let Some(desc) = desc else { return };
    if desc.is_empty() {
        return;
    }
    if desc.contains('\n') {
        out.push_str(indent);
        out.push_str("\"\"\"\n");
        for line in desc.lines() {
            out.push_str(indent);
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(indent);
        out.push_str("\"\"\"\n");
    } else {
        out.push_str(indent);
        out.push('"');
        out.push_str(desc);
        out.push_str("\"\n");
    }
}

fn print_scalar(out: &mut String, td: &TypeDef) {
    out.push_str("scalar ");
    out.push_str(&td.name);
    print_directives_inline(out, &td.directives);
    out.push('\n');
}

fn print_enum(out: &mut String, td: &TypeDef) {
    out.push_str("enum ");
    out.push_str(&td.name);
    print_directives_inline(out, &td.directives);
    out.push_str(" {\n");
    for value in &td.enum_values {
        print_description(out, value.description.as_deref(), "  ");
        out.push_str("  ");
        out.push_str(&value.name);
        if value.is_deprecated {
            out.push_str(" @deprecated");
            if let Some(ref reason) = value.deprecation_reason {
                out.push_str("(reason: \"");
                out.push_str(reason);
                out.push_str("\")");
            }
        }
        print_directives_inline(out, &value.directives);
        out.push('\n');
    }
    out.push_str("}\n");
}

fn print_union(out: &mut String, td: &TypeDef) {
    out.push_str("union ");
    out.push_str(&td.name);
    print_directives_inline(out, &td.directives);
    if !td.union_members.is_empty() {
        out.push_str(" = ");
        for (i, member) in td.union_members.iter().enumerate() {
            if i > 0 {
                out.push_str(" | ");
            }
            out.push_str(member);
        }
    }
    out.push('\n');
}

fn print_object(out: &mut String, td: &TypeDef, keyword: &str) {
    out.push_str(keyword);
    out.push(' ');
    out.push_str(&td.name);
    if !td.implements.is_empty() {
        out.push_str(" implements ");
        for (i, iface) in td.implements.iter().enumerate() {
            if i > 0 {
                out.push_str(" & ");
            }
            out.push_str(iface);
        }
    }
    print_directives_inline(out, &td.directives);
    out.push_str(" {\n");
    for field in &td.fields {
        print_field(out, field);
    }
    out.push_str("}\n");
}

fn print_input_object(out: &mut String, td: &TypeDef) {
    out.push_str("input ");
    out.push_str(&td.name);
    print_directives_inline(out, &td.directives);
    out.push_str(" {\n");
    for field in &td.fields {
        print_description(out, field.description.as_deref(), "  ");
        out.push_str("  ");
        out.push_str(&field.name);
        out.push_str(": ");
        out.push_str(&format_type_ref(&field.type_ref));
        print_directives_inline(out, &field.directives);
        out.push('\n');
    }
    out.push_str("}\n");
}

fn print_field(out: &mut String, field: &graphql_hir::FieldSignature) {
    print_description(out, field.description.as_deref(), "  ");
    out.push_str("  ");
    out.push_str(&field.name);
    if !field.arguments.is_empty() {
        out.push('(');
        for (i, arg) in field.arguments.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&arg.name);
            out.push_str(": ");
            out.push_str(&format_type_ref(&arg.type_ref));
            if let Some(ref default) = arg.default_value {
                out.push_str(" = ");
                out.push_str(default);
            }
        }
        out.push(')');
    }
    out.push_str(": ");
    out.push_str(&format_type_ref(&field.type_ref));
    if field.is_deprecated {
        out.push_str(" @deprecated");
        if let Some(ref reason) = field.deprecation_reason {
            out.push_str("(reason: \"");
            out.push_str(reason);
            out.push_str("\")");
        }
    }
    print_directives_inline(out, &field.directives);
    out.push('\n');
}

fn print_directives_inline(out: &mut String, directives: &[graphql_hir::DirectiveUsage]) {
    for dir in directives {
        // Skip @deprecated — handled explicitly
        if dir.name.as_ref() == "deprecated" {
            continue;
        }
        out.push_str(" @");
        out.push_str(&dir.name);
        if !dir.arguments.is_empty() {
            out.push('(');
            for (i, arg) in dir.arguments.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&arg.name);
                out.push_str(": ");
                out.push_str(&arg.value);
            }
            out.push(')');
        }
    }
}

fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let mut result = type_ref.name.to_string();

    if type_ref.is_list {
        if type_ref.inner_non_null {
            result.push('!');
        }
        result = format!("[{result}]");
    }

    if type_ref.is_non_null {
        result.push('!');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_scalars_excluded() {
        // Verify the constant is correct
        assert!(BUILTIN_SCALARS.contains(&"String"));
        assert!(BUILTIN_SCALARS.contains(&"ID"));
        assert!(!BUILTIN_SCALARS.contains(&"DateTime"));
    }
}
