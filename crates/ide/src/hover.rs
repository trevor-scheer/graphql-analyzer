//! Hover feature implementation.
//!
//! This module provides IDE hover functionality including:
//! - Field type and description information
//! - Type kind and description
//! - Fragment type condition
//! - Field usage counts and deprecation info

use std::fmt::Write as _;
use std::sync::Arc;

use crate::helpers::{find_block_for_position, format_type_ref, position_to_offset};
use crate::symbol::{find_parent_type_at_offset, find_symbol_at_offset, Symbol};
use crate::types::{FilePath, HoverResult, Position};
use crate::DbFiles;

/// Get hover information at a position.
///
/// Returns documentation, type information, etc.
pub fn hover(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: DbFiles<'_>,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
) -> Option<HoverResult> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;
        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);

    let (block_context, adjusted_position) = find_block_for_position(&parse, position)?;

    tracing::debug!(
        "Hover: original position {:?}, block line_offset {}, adjusted position {:?}",
        position,
        block_context.line_offset,
        adjusted_position
    );

    let block_line_index = graphql_syntax::LineIndex::new(block_context.block_source);
    let offset = position_to_offset(&block_line_index, adjusted_position)?;

    let symbol = find_symbol_at_offset(block_context.tree, offset);

    if symbol.is_none() && parse.has_errors() {
        let error_messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
        return Some(HoverResult::new(format!(
            "**Syntax Errors**\n\n{}",
            error_messages.join("\n")
        )));
    }

    let symbol = symbol?;
    let project_files = project_files?;

    match symbol {
        Symbol::FieldName { name } => {
            let types = graphql_hir::schema_types(db, project_files);
            let source_types = graphql_hir::source_schema_types(db, project_files);

            let parent_type_name =
                if let Some(parent_ctx) = find_parent_type_at_offset(block_context.tree, offset) {
                    crate::symbol::walk_type_stack_to_offset(
                        block_context.tree,
                        types,
                        offset,
                        &parent_ctx.root_type,
                    )?
                } else {
                    crate::symbol::find_schema_field_parent_type(block_context.tree, offset)?
                };

            tracing::debug!(
                "Hover: resolved parent type '{}' for field '{}'",
                parent_type_name,
                name
            );

            let parent_type = types.get(parent_type_name.as_str())?;
            let field = parent_type
                .fields
                .iter()
                .find(|f| f.name.as_ref() == name)?;

            // Check if field only exists in the resolved schema
            let from_resolved = graphql_hir::has_resolved_schema(db, project_files)
                && source_types
                    .get(parent_type_name.as_str())
                    .and_then(|td| td.fields.iter().find(|f| f.name.as_ref() == name))
                    .is_none();

            let mut hover_text = format!("**Field:** `{name}`\n\n");
            if from_resolved {
                write!(hover_text, "*(resolved schema)*\n\n").ok();
            }
            let field_type = format_type_ref(&field.type_ref);
            write!(hover_text, "**Type:** `{field_type}`\n\n").ok();

            let type_usages = graphql_analysis::field_usage_for_type(
                db,
                project_files,
                Arc::from(parent_type_name.as_str()),
            );
            if let Some(usage) = type_usages.get(name.as_str()) {
                let op_count = usage.operations.len();
                if op_count > 0 {
                    write!(
                        hover_text,
                        "**Used in:** {op_count} operation{}\n\n",
                        if op_count == 1 { "" } else { "s" }
                    )
                    .ok();
                } else {
                    write!(hover_text, "**Used in:** 0 operations (unused)\n\n").ok();
                }
            }

            if let Some(desc) = &field.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            if field.is_deprecated {
                write!(hover_text, "---\n\n").ok();
                if let Some(reason) = &field.deprecation_reason {
                    write!(hover_text, "**Deprecated:** {reason}\n\n").ok();
                } else {
                    write!(hover_text, "**Deprecated**\n\n").ok();
                }
            }

            Some(HoverResult::new(hover_text))
        }
        Symbol::TypeName { name } => {
            let types = graphql_hir::schema_types(db, project_files);
            let type_def = types.get(name.as_str())?;

            let mut hover_text = format!("**Type:** `{name}`\n\n");
            let kind_str = match type_def.kind {
                graphql_hir::TypeDefKind::Object => "Object",
                graphql_hir::TypeDefKind::Interface => "Interface",
                graphql_hir::TypeDefKind::Union => "Union",
                graphql_hir::TypeDefKind::Enum => "Enum",
                graphql_hir::TypeDefKind::Scalar => "Scalar",
                graphql_hir::TypeDefKind::InputObject => "Input Object",
                _ => "Unknown",
            };
            write!(hover_text, "**Kind:** {kind_str}\n\n").ok();

            if let Some(desc) = &type_def.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            Some(HoverResult::new(hover_text))
        }
        Symbol::FragmentSpread { name } => {
            let fragments = graphql_hir::all_fragments(db, project_files);
            let fragment = fragments.get(name.as_str())?;

            let hover_text = format!(
                "**Fragment:** `{}`\n\n**On Type:** `{}`\n\n",
                name, fragment.type_condition
            );

            Some(HoverResult::new(hover_text))
        }
        Symbol::DirectiveName { name } => {
            let directives = graphql_hir::schema_directives(db, project_files);
            let directive = directives.get(name.as_str())?;

            let mut hover_text = format!("**Directive:** `@{name}`\n\n");

            let locations: Vec<&str> = directive
                .locations
                .iter()
                .map(|l| format_directive_location(l))
                .collect();
            write!(hover_text, "**Locations:** {}\n\n", locations.join(" | ")).ok();

            if directive.repeatable {
                write!(hover_text, "**Repeatable:** yes\n\n").ok();
            }

            if !directive.arguments.is_empty() {
                write!(hover_text, "**Arguments:**\n\n").ok();
                for arg in &directive.arguments {
                    let type_str = format_type_ref(&arg.type_ref);
                    if let Some(default) = &arg.default_value {
                        write!(hover_text, "- `{}: {} = {}`\n", arg.name, type_str, default)
                            .ok();
                    } else {
                        write!(hover_text, "- `{}: {}`\n", arg.name, type_str).ok();
                    }
                }
                write!(hover_text, "\n").ok();
            }

            if let Some(desc) = &directive.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            Some(HoverResult::new(hover_text))
        }
        Symbol::DirectiveArgumentName {
            directive_name,
            argument_name,
        } => {
            let directives = graphql_hir::schema_directives(db, project_files);
            let directive = directives.get(directive_name.as_str())?;
            let arg = directive
                .arguments
                .iter()
                .find(|a| a.name.as_ref() == argument_name)?;

            let type_str = format_type_ref(&arg.type_ref);
            let mut hover_text = format!("**Argument:** `{argument_name}: {type_str}`\n\n");
            write!(hover_text, "**Directive:** `@{directive_name}`\n\n").ok();

            if let Some(default) = &arg.default_value {
                write!(hover_text, "**Default:** `{default}`\n\n").ok();
            }

            if let Some(desc) = &arg.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            Some(HoverResult::new(hover_text))
        }
        _ => Some(HoverResult::new(format!("Symbol: {symbol:?}"))),
    }
}

pub(crate) fn format_directive_location(
    location: &graphql_hir::DirectiveLocationKind,
) -> &'static str {
    use graphql_hir::DirectiveLocationKind;
    match location {
        DirectiveLocationKind::Query => "QUERY",
        DirectiveLocationKind::Mutation => "MUTATION",
        DirectiveLocationKind::Subscription => "SUBSCRIPTION",
        DirectiveLocationKind::Field => "FIELD",
        DirectiveLocationKind::FragmentDefinition => "FRAGMENT_DEFINITION",
        DirectiveLocationKind::FragmentSpread => "FRAGMENT_SPREAD",
        DirectiveLocationKind::InlineFragment => "INLINE_FRAGMENT",
        DirectiveLocationKind::VariableDefinition => "VARIABLE_DEFINITION",
        DirectiveLocationKind::Schema => "SCHEMA",
        DirectiveLocationKind::Scalar => "SCALAR",
        DirectiveLocationKind::Object => "OBJECT",
        DirectiveLocationKind::FieldDefinition => "FIELD_DEFINITION",
        DirectiveLocationKind::ArgumentDefinition => "ARGUMENT_DEFINITION",
        DirectiveLocationKind::Interface => "INTERFACE",
        DirectiveLocationKind::Union => "UNION",
        DirectiveLocationKind::Enum => "ENUM",
        DirectiveLocationKind::EnumValue => "ENUM_VALUE",
        DirectiveLocationKind::InputObject => "INPUT_OBJECT",
        DirectiveLocationKind::InputFieldDefinition => "INPUT_FIELD_DEFINITION",
    }
}
