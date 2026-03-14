use super::{AnalysisHost, DiagnosticSeverity, DocumentKind, FilePath, Language};

/// Helper to set up a host with schema + document files and rebuild project files
fn setup_host(
    schema: &str,
    files: &[(&str, &str, DocumentKind)],
) -> (AnalysisHost, FilePath, Vec<FilePath>) {
    let mut host = AnalysisHost::new();

    let schema_path = FilePath::new("file:///project/schema.graphql");
    host.add_file(
        &schema_path,
        schema,
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let mut file_paths = Vec::new();
    for (name, content, kind) in files {
        let path = FilePath::new(format!("file:///project/{name}"));
        host.add_file(&path, content, Language::GraphQL, *kind);
        file_paths.push(path);
    }

    host.rebuild_project_files();
    (host, schema_path, file_paths)
}

fn has_error(diagnostics: &[super::Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error)
}

#[test]
fn schema_field_rename_propagates_to_operations() {
    let (mut host, schema_path, file_paths) = setup_host(
        "type Query { hero: String }",
        &[("query.graphql", "query { hero }", DocumentKind::Executable)],
    );
    let query_path = &file_paths[0];

    // Initial state: no errors
    {
        let snapshot = host.snapshot();
        assert!(!has_error(&snapshot.diagnostics(query_path)));
    }

    // Rename schema field: hero -> heroes
    host.add_file(
        &schema_path,
        "type Query { heroes: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // diagnostics_for_change on the schema file should include query.graphql errors
    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);
        assert!(
            result.contains_key(query_path),
            "query.graphql should be in affected files"
        );
        assert!(
            has_error(result.get(query_path).unwrap()),
            "query.graphql should now have an error"
        );
    }
}

#[test]
fn schema_field_rename_does_not_affect_unrelated_files() {
    let (mut host, schema_path, file_paths) = setup_host(
        "type Query { hero: String\nvillain: String }",
        &[
            (
                "hero_query.graphql",
                "query { hero }",
                DocumentKind::Executable,
            ),
            (
                "villain_query.graphql",
                "query { villain }",
                DocumentKind::Executable,
            ),
        ],
    );
    let hero_path = &file_paths[0];
    let villain_path = &file_paths[1];

    // Initial state: no errors
    {
        let snapshot = host.snapshot();
        assert!(!has_error(&snapshot.diagnostics(hero_path)));
        assert!(!has_error(&snapshot.diagnostics(villain_path)));
    }

    // Rename hero -> heroes (villain is untouched)
    host.add_file(
        &schema_path,
        "type Query { heroes: String\nvillain: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // hero_query should now have errors
        assert!(has_error(result.get(hero_path).unwrap()));

        // villain_query should still be clean (Salsa revalidates it but finds no errors)
        let villain_diags = result.get(villain_path).unwrap();
        assert!(!has_error(villain_diags));
    }
}

#[test]
fn fragment_rename_propagates_to_spreaders() {
    let (mut host, _schema_path, file_paths) = setup_host(
        "type Query { hero: String }",
        &[
            (
                "fragment.graphql",
                "fragment HeroFields on Query { hero }",
                DocumentKind::Executable,
            ),
            (
                "operation.graphql",
                "query { ...HeroFields }",
                DocumentKind::Executable,
            ),
        ],
    );
    let fragment_path = &file_paths[0];
    let operation_path = &file_paths[1];

    // Initial state: no errors
    {
        let snapshot = host.snapshot();
        assert!(!has_error(&snapshot.diagnostics(operation_path)));
    }

    // Rename fragment: HeroFields -> HeroData
    host.add_file(
        fragment_path,
        "fragment HeroData on Query { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // diagnostics_for_change should flag operation.graphql
    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(fragment_path);

        // operation.graphql should be affected (it spreads HeroFields which no longer exists)
        assert!(
            result.contains_key(operation_path),
            "operation.graphql should be in affected files"
        );
    }
}

#[test]
fn operation_body_change_does_not_trigger_cross_file_refresh() {
    let (mut host, _schema_path, file_paths) = setup_host(
        "type Query { hero: String\nvillain: String }",
        &[
            (
                "query_a.graphql",
                "query A { hero }",
                DocumentKind::Executable,
            ),
            (
                "query_b.graphql",
                "query B { villain }",
                DocumentKind::Executable,
            ),
        ],
    );
    let query_a = &file_paths[0];
    let query_b = &file_paths[1];

    // Edit query A's body (no name change, no fragment change)
    host.add_file(
        query_a,
        "query A { hero\nvillain }",
        Language::GraphQL,
        DocumentKind::Executable,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(query_a);

        // Should only contain query_a itself, not query_b
        assert!(result.contains_key(query_a));
        assert!(
            !result.contains_key(query_b),
            "query_b should NOT be affected by query_a body edit"
        );
    }
}

#[test]
fn operation_name_collision_propagates() {
    let (mut host, _schema_path, file_paths) = setup_host(
        "type Query { hero: String }",
        &[
            (
                "query_a.graphql",
                "query GetHero { hero }",
                DocumentKind::Executable,
            ),
            (
                "query_b.graphql",
                "query GetVillain { hero }",
                DocumentKind::Executable,
            ),
        ],
    );
    let query_a = &file_paths[0];
    let query_b = &file_paths[1];

    // Rename query B to collide with query A
    host.add_file(
        query_b,
        "query GetHero { hero }",
        Language::GraphQL,
        DocumentKind::Executable,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(query_b);

        // query_a should be affected (now has a name collision)
        assert!(
            result.contains_key(query_a),
            "query_a should be affected by name collision"
        );
    }
}

#[test]
fn document_change_without_fragments_or_ops_is_self_only() {
    // File with anonymous operation - no named ops, no fragments
    let (mut host, _schema_path, file_paths) = setup_host(
        "type Query { hero: String\nvillain: String }",
        &[
            ("anon.graphql", "{ hero }", DocumentKind::Executable),
            ("other.graphql", "{ villain }", DocumentKind::Executable),
        ],
    );
    let anon_path = &file_paths[0];
    let other_path = &file_paths[1];

    host.add_file(
        anon_path,
        "{ hero\nvillain }",
        Language::GraphQL,
        DocumentKind::Executable,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(anon_path);
        assert!(result.contains_key(anon_path));
        assert!(!result.contains_key(other_path));
    }
}

#[test]
fn orphan_extensions_produce_valid_schema_for_document_validation() {
    // Schema defined entirely via `extend type` (no base `type Query` definition).
    // Without adopt_orphan_extensions(), SchemaBuilder::build() would fail and
    // document validation would silently return zero diagnostics for everything.
    let (host, _schema_path, file_paths) = setup_host(
        "extend type Query { hero: String }",
        &[("query.graphql", "query { hero }", DocumentKind::Executable)],
    );
    let query_path = &file_paths[0];

    let snapshot = host.snapshot();
    let diagnostics = snapshot.diagnostics(query_path);

    // The query is valid against the schema — no errors expected.
    assert!(
        !has_error(&diagnostics),
        "valid query against orphan-extension schema should have no errors, got: {diagnostics:?}"
    );
}

#[test]
fn orphan_extensions_detect_invalid_fields() {
    // Verify that validation actually runs against orphan-extension schemas
    // (not just silently returning empty diagnostics).
    let (host, _schema_path, file_paths) = setup_host(
        "extend type Query { hero: String }",
        &[(
            "query.graphql",
            "query { nonexistent }",
            DocumentKind::Executable,
        )],
    );
    let query_path = &file_paths[0];

    let snapshot = host.snapshot();
    let diagnostics = snapshot.diagnostics(query_path);

    assert!(
        has_error(&diagnostics),
        "querying nonexistent field against orphan-extension schema should produce an error"
    );
}
