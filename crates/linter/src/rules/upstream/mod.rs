//! Verbatim ports of `@graphql-eslint`'s unit tests, run as Rust unit tests
//! against this crate's lint rule implementations.
//!
//! Each submodule is one ported rule; cases inside each module are individual
//! `#[test] fn`s named `{valid,invalid}_l<lineno>_<slug>` with a permalink
//! doc-comment pointing at the original upstream test line.
//!
//! All permalinks reference a single pinned SHA recorded in [`UPSTREAM_SHA`].
//! See `docs/superpowers/specs/2026-04-28-port-graphql-eslint-tests-design.md`
//! for the rationale and what's intentionally out of scope.

/// Upstream `dimaMachina/graphql-eslint` SHA all ported cases pin to.
/// Recorded once at start of port; never refreshed (see spec).
pub(crate) const UPSTREAM_SHA: &str = "f0f200ef0b030cb8a905bbcb32fe346b87cc2e24";

mod alphabetize;
mod description_style;
pub(crate) mod harness;
mod input_name;
mod lone_executable_definition;
mod match_document_filename;
mod naming_convention;
mod no_anonymous_operations;
mod no_deprecated;
mod no_duplicate_fields;
mod no_hashtag_description;
mod no_one_place_fragments;
mod no_root_type;
mod no_scalar_result_type_on_mutation;
mod no_typename_prefix;
mod no_unreachable_types;
mod no_unused_fields;
mod no_unused_fragments;
mod no_unused_variables;
mod relay_arguments;
mod relay_connection_types;
mod relay_edge_types;
mod relay_page_info;
mod require_deprecation_date;
