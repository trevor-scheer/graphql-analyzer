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

#![cfg(test)]

/// Upstream `dimaMachina/graphql-eslint` SHA all ported cases pin to.
/// Recorded once at start of port; never refreshed (see spec).
// No rule submodules exist yet; they'll reference this once ported.
#[allow(dead_code)]
pub(crate) const UPSTREAM_SHA: &str = "f0f200ef0b030cb8a905bbcb32fe346b87cc2e24";

pub(crate) mod harness;
pub(crate) mod no_anonymous_operations;
