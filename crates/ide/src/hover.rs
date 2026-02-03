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
use crate::FileRegistry;

/// Get hover information at a position.
///
/// Returns documentation, type information, etc.
pub fn hover(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
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

            let mut hover_text = format!("**Field:** `{name}`\n\n");
            let field_type = format_type_ref(&field.type_ref);
            write!(hover_text, "**Type:** `{field_type}`\n\n").ok();

            let coverage = graphql_analysis::analyze_field_usage(db, project_files);
            let usage_key = (Arc::from(parent_type_name.as_str()), Arc::from(name));
            if let Some(usage) = coverage.field_usages.get(&usage_key) {
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
        _ => Some(HoverResult::new(format!("Symbol: {symbol:?}"))),
    }
}
