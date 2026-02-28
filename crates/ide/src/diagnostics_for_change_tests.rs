use super::{AnalysisHost, DiagnosticSeverity, DocumentKind, FilePath, Language};

#[test]
fn schema_change_returns_document_diagnostics() {
    let mut host = AnalysisHost::new();

    let schema_path = FilePath::new("file:///project/schema.graphql");
    let query_path = FilePath::new("file:///project/query.graphql");

    // Start with a valid schema + query
    host.add_file(
        &schema_path,
        "type Query { hero: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );
    host.add_file(
        &query_path,
        "query { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host.rebuild_project_files();

    // Query should be valid initially
    {
        let snapshot = host.snapshot();
        let diags = snapshot.diagnostics(&query_path);
        assert!(
            diags
                .iter()
                .all(|d| d.severity != DiagnosticSeverity::Error),
            "query should be valid before schema change"
        );
    }

    // Rename schema field: hero -> heroes (breaks the query)
    host.add_file(
        &schema_path,
        "type Query { heroes: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );
    host.rebuild_project_files();

    // diagnostics_for_change on the schema file should return diagnostics
    // for both the schema file AND the document file
    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // Must include diagnostics for the schema file itself
        assert!(
            result.contains_key(&schema_path),
            "result should include the changed schema file"
        );

        // Must include diagnostics for the document file
        assert!(
            result.contains_key(&query_path),
            "result should include document files when schema changes"
        );

        // The query file should now have errors (field 'hero' no longer exists)
        let query_diags = &result[&query_path];
        assert!(
            query_diags
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error),
            "query should have errors after schema field rename"
        );
    }
}

#[test]
fn document_change_returns_only_changed_file() {
    let mut host = AnalysisHost::new();

    let schema_path = FilePath::new("file:///project/schema.graphql");
    let query1_path = FilePath::new("file:///project/query1.graphql");
    let query2_path = FilePath::new("file:///project/query2.graphql");

    host.add_file(
        &schema_path,
        "type Query { hero: String, villain: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );
    host.add_file(
        &query1_path,
        "query { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host.add_file(
        &query2_path,
        "query { villain }",
        Language::GraphQL,
        DocumentKind::Executable,
    );
    host.rebuild_project_files();

    // Changing a document file should only return diagnostics for that file
    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&query1_path);

        assert!(
            result.contains_key(&query1_path),
            "result should include the changed file"
        );
        assert_eq!(
            result.len(),
            1,
            "changing a document file should not re-validate other files"
        );
    }
}
