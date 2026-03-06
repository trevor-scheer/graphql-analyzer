//! Signature help implementation.
//!
//! Shows argument information when the cursor is inside field or directive
//! argument lists: which arguments are available, their types, and which
//! one is currently being filled in.

use crate::helpers::{
    find_argument_context_at_offset, find_block_for_position,
    find_directive_argument_context_at_offset, format_type_ref, position_to_offset,
};
use crate::symbol::find_parent_type_at_offset;
use crate::types::{FilePath, ParameterInformation, Position, SignatureHelp, SignatureInformation};
use crate::FileRegistry;

/// Get signature help at a position.
///
/// Returns signature information when the cursor is inside a field or
/// directive argument list.
pub fn signature_help(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
) -> Option<SignatureHelp> {
    let project_files = project_files?;

    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;
        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    // Try directive arguments first (more specific context)
    if let Some(help) = try_directive_signature_help(
        db,
        project_files,
        block_context.tree,
        offset,
        block_context.block_source,
    ) {
        return Some(help);
    }

    // Try field arguments
    try_field_signature_help(
        db,
        project_files,
        block_context.tree,
        offset,
        block_context.block_source,
    )
}

/// Build signature help for a field's arguments.
fn try_field_signature_help(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    source: &str,
) -> Option<SignatureHelp> {
    let arg_ctx = find_argument_context_at_offset(tree, offset)?;

    let types = graphql_hir::schema_types(db, project_files);
    let parent_ctx = find_parent_type_at_offset(tree, offset)?;
    let parent_type_name =
        crate::symbol::walk_type_stack_to_offset(tree, types, offset, &parent_ctx.root_type)?;

    let parent_type = types.get(parent_type_name.as_str())?;
    let field_def = parent_type
        .fields
        .iter()
        .find(|f| f.name.as_ref() == arg_ctx.field_name)?;

    // Don't show signature help for fields with no arguments
    if field_def.arguments.is_empty() {
        return None;
    }

    let return_type = format_type_ref(&field_def.type_ref);
    let (label, parameters) = build_signature_label_and_params(
        &arg_ctx.field_name,
        &field_def.arguments,
        Some(&return_type),
    );

    let active_parameter = count_active_parameter(tree, offset, source);

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: field_def
                .description
                .as_ref()
                .map(std::string::ToString::to_string),
            parameters,
        }],
        active_signature: Some(0),
        active_parameter,
    })
}

/// Build signature help for a directive's arguments.
fn try_directive_signature_help(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    source: &str,
) -> Option<SignatureHelp> {
    let dir_ctx = find_directive_argument_context_at_offset(tree, offset)?;

    let directives = graphql_hir::schema_directives(db, project_files);
    let dir_def = directives.get(dir_ctx.directive_name.as_str())?;

    if dir_def.arguments.is_empty() {
        return None;
    }

    let directive_label = format!("@{}", dir_ctx.directive_name);
    let (label, parameters) =
        build_signature_label_and_params(&directive_label, &dir_def.arguments, None);

    let active_parameter = count_active_parameter(tree, offset, source);

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: dir_def
                .description
                .as_ref()
                .map(std::string::ToString::to_string),
            parameters,
        }],
        active_signature: Some(0),
        active_parameter,
    })
}

/// Build a signature label string and parameter offset ranges.
///
/// For fields: `fieldName(arg1: Type1, arg2: Type2): ReturnType`
/// For directives: `@directiveName(arg1: Type1, arg2: Type2)`
fn build_signature_label_and_params(
    name: &str,
    arguments: &[graphql_hir::ArgumentDef],
    return_type: Option<&str>,
) -> (String, Vec<ParameterInformation>) {
    let mut label = format!("{name}(");
    let mut parameters = Vec::with_capacity(arguments.len());

    for (i, arg) in arguments.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }

        let param_start = label.len() as u32;
        let type_str = format_type_ref(&arg.type_ref);
        let param_text = if let Some(default) = &arg.default_value {
            format!("{}: {} = {}", arg.name, type_str, default)
        } else {
            format!("{}: {}", arg.name, type_str)
        };
        label.push_str(&param_text);
        let param_end = label.len() as u32;

        parameters.push(ParameterInformation {
            label_offsets: (param_start, param_end),
            documentation: arg
                .description
                .as_ref()
                .map(std::string::ToString::to_string),
        });
    }

    label.push(')');

    if let Some(ret) = return_type {
        label.push_str(": ");
        label.push_str(ret);
    }

    (label, parameters)
}

/// Count commas before the cursor within the argument list to determine the active parameter.
///
/// Walks the CST to find the arguments node containing the cursor, then counts
/// commas between the opening `(` and the cursor position.
fn count_active_parameter(
    tree: &apollo_parser::SyntaxTree,
    offset: usize,
    source: &str,
) -> Option<u32> {
    // Find the arguments node that contains our offset
    let args_range = find_arguments_range(tree, offset)?;
    let args_start: usize = args_range.0;

    // Count commas between `(` and cursor, but not inside nested parens/braces/brackets
    let slice = source.get(args_start..offset)?;
    let mut depth = 0i32;
    let mut commas = 0u32;

    for byte in slice.bytes() {
        match byte {
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            // Only count commas at the argument list level (depth 1 = inside the parens)
            b',' if depth == 1 => commas += 1,
            _ => {}
        }
    }

    Some(commas)
}

/// Find the byte range of the Arguments CST node containing the given offset.
fn find_arguments_range(tree: &apollo_parser::SyntaxTree, offset: usize) -> Option<(usize, usize)> {
    use apollo_parser::cst::{CstNode, Definition, Selection};

    fn check_arguments(
        args: &apollo_parser::cst::Arguments,
        offset: usize,
    ) -> Option<(usize, usize)> {
        let range = args.syntax().text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        if offset >= start && offset <= end {
            Some((start, end))
        } else {
            None
        }
    }

    fn check_directives(
        directives: &apollo_parser::cst::Directives,
        offset: usize,
    ) -> Option<(usize, usize)> {
        for directive in directives.directives() {
            if let Some(args) = directive.arguments() {
                if let Some(range) = check_arguments(&args, offset) {
                    return Some(range);
                }
            }
        }
        None
    }

    fn check_selection_set(
        selection_set: &apollo_parser::cst::SelectionSet,
        offset: usize,
    ) -> Option<(usize, usize)> {
        for selection in selection_set.selections() {
            match selection {
                Selection::Field(field) => {
                    if let Some(args) = field.arguments() {
                        if let Some(range) = check_arguments(&args, offset) {
                            return Some(range);
                        }
                    }
                    if let Some(directives) = field.directives() {
                        if let Some(range) = check_directives(&directives, offset) {
                            return Some(range);
                        }
                    }
                    if let Some(nested) = field.selection_set() {
                        if let Some(range) = check_selection_set(&nested, offset) {
                            return Some(range);
                        }
                    }
                }
                Selection::InlineFragment(inline_frag) => {
                    if let Some(directives) = inline_frag.directives() {
                        if let Some(range) = check_directives(&directives, offset) {
                            return Some(range);
                        }
                    }
                    if let Some(nested) = inline_frag.selection_set() {
                        if let Some(range) = check_selection_set(&nested, offset) {
                            return Some(range);
                        }
                    }
                }
                Selection::FragmentSpread(frag_spread) => {
                    if let Some(directives) = frag_spread.directives() {
                        if let Some(range) = check_directives(&directives, offset) {
                            return Some(range);
                        }
                    }
                }
            }
        }
        None
    }

    let doc = tree.document();
    for definition in doc.definitions() {
        match definition {
            Definition::OperationDefinition(op) => {
                if let Some(directives) = op.directives() {
                    if let Some(range) = check_directives(&directives, offset) {
                        return Some(range);
                    }
                }
                if let Some(selection_set) = op.selection_set() {
                    if let Some(range) = check_selection_set(&selection_set, offset) {
                        return Some(range);
                    }
                }
            }
            Definition::FragmentDefinition(frag) => {
                if let Some(directives) = frag.directives() {
                    if let Some(range) = check_directives(&directives, offset) {
                        return Some(range);
                    }
                }
                if let Some(selection_set) = frag.selection_set() {
                    if let Some(range) = check_selection_set(&selection_set, offset) {
                        return Some(range);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build signature help label and params for testing
    fn make_test_args(args: &[(&str, &str, Option<&str>)]) -> Vec<graphql_hir::ArgumentDef> {
        args.iter()
            .map(|(name, type_name, default)| {
                let non_null = type_name.ends_with('!');
                let base_name = type_name.trim_end_matches('!');
                graphql_hir::ArgumentDef {
                    name: (*name).into(),
                    type_ref: graphql_hir::TypeRef {
                        name: base_name.into(),
                        is_list: false,
                        is_non_null: non_null,
                        inner_non_null: false,
                    },
                    default_value: default.map(std::convert::Into::into),
                    description: None,
                    is_deprecated: false,
                    deprecation_reason: None,
                    directives: vec![],
                    name_range: graphql_hir::TextRange::new(0.into(), 0.into()),
                    file_id: graphql_base_db::FileId::new(0),
                }
            })
            .collect()
    }

    #[test]
    fn test_build_signature_label_field_with_return_type() {
        let args = make_test_args(&[("id", "ID!", None), ("name", "String", None)]);
        let (label, params) = build_signature_label_and_params("user", &args, Some("User"));

        assert_eq!(label, "user(id: ID!, name: String): User");
        assert_eq!(params.len(), 2);

        // Verify param offsets point to correct substrings
        let p0 = &params[0];
        assert_eq!(
            &label[p0.label_offsets.0 as usize..p0.label_offsets.1 as usize],
            "id: ID!"
        );
        let p1 = &params[1];
        assert_eq!(
            &label[p1.label_offsets.0 as usize..p1.label_offsets.1 as usize],
            "name: String"
        );
    }

    #[test]
    fn test_build_signature_label_directive() {
        let args = make_test_args(&[
            ("maxAge", "Int", None),
            ("scope", "CacheControlScope", None),
        ]);
        let (label, params) = build_signature_label_and_params("@cacheControl", &args, None);

        assert_eq!(
            label,
            "@cacheControl(maxAge: Int, scope: CacheControlScope)"
        );
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_signature_label_with_default_value() {
        let args = make_test_args(&[("first", "Int", Some("10")), ("after", "String", None)]);
        let (label, params) = build_signature_label_and_params("posts", &args, Some("[Post!]!"));

        assert_eq!(label, "posts(first: Int = 10, after: String): [Post!]!");
        let p0 = &params[0];
        assert_eq!(
            &label[p0.label_offsets.0 as usize..p0.label_offsets.1 as usize],
            "first: Int = 10"
        );
    }

    #[test]
    fn test_count_active_parameter_basic() {
        // `field(a: 1, b: |)` - 1 comma before cursor
        let source = "{ field(a: 1, b: 2) }";
        let parser = apollo_parser::Parser::new(source);
        let tree = parser.parse();

        // Cursor at `b: 2` -> after the comma
        let cursor = source.find("b: 2").unwrap();
        let active = count_active_parameter(&tree, cursor, source);
        assert_eq!(active, Some(1));
    }

    #[test]
    fn test_count_active_parameter_first_arg() {
        let source = "{ field(a: 1) }";
        let parser = apollo_parser::Parser::new(source);
        let tree = parser.parse();

        // Cursor inside first arg
        let cursor = source.find("a: 1").unwrap();
        let active = count_active_parameter(&tree, cursor, source);
        assert_eq!(active, Some(0));
    }

    #[test]
    fn test_count_active_parameter_empty_parens() {
        let source = "{ field() }";
        let parser = apollo_parser::Parser::new(source);
        let tree = parser.parse();

        // Cursor inside empty parens
        let cursor = source.find(')').unwrap();
        let active = count_active_parameter(&tree, cursor, source);
        assert_eq!(active, Some(0));
    }
}
