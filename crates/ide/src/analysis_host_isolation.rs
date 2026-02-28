//! Integration test for per-project `AnalysisHost` isolation in the LSP
//!
//! This test simulates two projects with different schemas and ensures their analysis is isolated.
//!
//! # Snapshot Lifecycle
//!
//! Salsa uses a single-writer, multi-reader model. Snapshots (via `Analysis`) hold read locks
//! on the database. All snapshots MUST be dropped before any mutating operation (`add_file`,
//! `rebuild_project_files`, etc.) or the mutation will hang waiting for the read locks.

use super::{AnalysisHost, DiagnosticSeverity, DocumentKind, FilePath, Language};

#[test]
fn test_analysis_host_isolation_between_projects() {
    // Project 1: StarWars
    let mut host1 = AnalysisHost::new();
    let path1 = FilePath::new("file:///starwars/schema.graphql");
    host1.add_file(
        &path1,
        "type Query { hero: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );
    host1.rebuild_project_files();

    // Scope the snapshot so it's dropped before the next mutation
    {
        let snapshot1 = host1.snapshot();
        let diagnostics1 = snapshot1.diagnostics(&path1);
        assert!(diagnostics1
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    // Project 2: Pokemon
    let mut host2 = AnalysisHost::new();
    let path2 = FilePath::new("file:///pokemon/schema.graphql");
    host2.add_file(
        &path2,
        "type Query { pokemon: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );
    host2.rebuild_project_files();

    {
        let snapshot2 = host2.snapshot();
        let diagnostics2 = snapshot2.diagnostics(&path2);
        assert!(diagnostics2
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    // Add a file to project 1 that would be invalid in project 2
    let file1 = FilePath::new("file:///starwars/query.graphql");
    host1.add_file(
        &file1,
        "query { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host1.rebuild_project_files();

    {
        let snapshot1b = host1.snapshot();
        let diagnostics1b = snapshot1b.diagnostics(&file1);
        assert!(diagnostics1b
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    // Add a file to project 2 that would be invalid in project 1
    let file2 = FilePath::new("file:///pokemon/query.graphql");
    host2.add_file(
        &file2,
        "query { pokemon }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host2.rebuild_project_files();

    {
        let snapshot2b = host2.snapshot();
        let diagnostics2b = snapshot2b.diagnostics(&file2);
        assert!(diagnostics2b
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    // Cross-check: project 1 should not recognize 'pokemon', project 2 should not recognize 'hero'
    let file1_invalid = FilePath::new("file:///starwars/query_pokemon.graphql");
    host1.add_file(
        &file1_invalid,
        "query { pokemon }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host1.rebuild_project_files();

    {
        let snapshot1c = host1.snapshot();
        let diagnostics1c = snapshot1c.diagnostics(&file1_invalid);
        assert!(diagnostics1c
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error));
    }

    let file2_invalid = FilePath::new("file:///pokemon/query_hero.graphql");
    host2.add_file(
        &file2_invalid,
        "query { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host2.rebuild_project_files();

    {
        let snapshot2c = host2.snapshot();
        let diagnostics2c = snapshot2c.diagnostics(&file2_invalid);
        assert!(diagnostics2c
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error));
    }
}
