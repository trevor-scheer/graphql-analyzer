use super::{AnalysisHost, DiagnosticSeverity, DocumentKind, FilePath, Language};

/// Helper to set up a host with fingerprints initialized.
/// This mirrors what the LSP does: load files, rebuild, capture fingerprints.
fn setup_host_with_fingerprints(
    schema: &str,
    files: &[(&str, &str, DocumentKind)],
) -> (AnalysisHost, FilePath, Vec<FilePath>) {
    let (mut host, schema_path, file_paths) = setup_host(schema, files);
    host.update_schema_fingerprints();
    (host, schema_path, file_paths)
}

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

// ============================================================================
// Schema-diff filtering tests
// ============================================================================

#[test]
fn schema_diff_skips_unrelated_type_change() {
    // Two operations reference different types via fragments.
    // Changing only one type should only re-validate the operation
    // that references it (via fragment type condition).
    let (mut host, schema_path, file_paths) = setup_host_with_fingerprints(
        "type Query { user: User\npost: Post }\ntype User { name: String }\ntype Post { title: String }",
        &[
            (
                "user_query.graphql",
                "fragment UserFields on User { name }\nquery { user { ...UserFields } }",
                DocumentKind::Executable,
            ),
            (
                "post_query.graphql",
                "fragment PostFields on Post { title }\nquery { post { ...PostFields } }",
                DocumentKind::Executable,
            ),
        ],
    );
    let user_query = &file_paths[0];
    let post_query = &file_paths[1];

    // Initial: no errors
    {
        let snapshot = host.snapshot();
        assert!(!has_error(&snapshot.diagnostics(user_query)));
        assert!(!has_error(&snapshot.diagnostics(post_query)));
    }

    // Change only the User type (add a field)
    host.add_file(
        &schema_path,
        "type Query { user: User\npost: Post }\ntype User { name: String\nemail: String }\ntype Post { title: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // user_query references User so it SHOULD be re-validated
        assert!(
            result.contains_key(user_query),
            "user_query should be re-validated (references changed User type)"
        );

        // post_query only references Post which didn't change - should be skipped
        assert!(
            !result.contains_key(post_query),
            "post_query should be skipped (Post type unchanged)"
        );
    }
}

#[test]
fn schema_diff_revalidates_all_when_query_type_changes() {
    // When the Query root type changes, all operations must be re-validated
    // because operations implicitly depend on their root type.
    let (mut host, schema_path, file_paths) = setup_host_with_fingerprints(
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

    // Add a new field to Query type
    host.add_file(
        &schema_path,
        "type Query { hero: String\nvillain: String\nsidekick: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // Both should be re-validated when Query type changes
        assert!(
            result.contains_key(hero_path),
            "hero_query should be re-validated (Query type changed)"
        );
        assert!(
            result.contains_key(villain_path),
            "villain_query should be re-validated (Query type changed)"
        );
    }
}

#[test]
fn schema_diff_skips_all_when_no_types_changed() {
    // If a schema file changes but no type definitions actually changed
    // (e.g., whitespace or comment change), all document files should be skipped.
    let (mut host, schema_path, file_paths) = setup_host_with_fingerprints(
        "type Query { hero: String }",
        &[("query.graphql", "query { hero }", DocumentKind::Executable)],
    );
    let query_path = &file_paths[0];

    // Initial: no errors
    {
        let snapshot = host.snapshot();
        assert!(!has_error(&snapshot.diagnostics(query_path)));
    }

    // "Change" the schema by adding a description (which changes the SDL
    // but might or might not change type fingerprints depending on whether
    // descriptions are part of TypeDef). Even if it does change, the key
    // test is that the diffing mechanism works.
    //
    // Use exact same content to test the "no types changed" path.
    host.add_file(
        &schema_path,
        "type Query { hero: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // Only the schema file itself should be in the result
        assert!(result.contains_key(&schema_path));
        assert!(
            !result.contains_key(query_path),
            "query.graphql should be skipped (no types changed)"
        );
    }
}

#[test]
fn schema_diff_handles_type_removal() {
    // When a type is removed from the schema, operations referencing it
    // should be re-validated.
    let (mut host, schema_path, file_paths) = setup_host_with_fingerprints(
        "type Query { user: User }\ntype User { name: String }\ntype Post { title: String }",
        &[
            (
                "user_frag.graphql",
                "fragment F on User { name }",
                DocumentKind::Executable,
            ),
            (
                "post_frag.graphql",
                "fragment P on Post { title }",
                DocumentKind::Executable,
            ),
        ],
    );
    let user_frag = &file_paths[0];
    let post_frag = &file_paths[1];

    // Remove the Post type
    host.add_file(
        &schema_path,
        "type Query { user: User }\ntype User { name: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // post_frag references Post which was removed - should be re-validated
        assert!(
            result.contains_key(post_frag),
            "post_frag should be re-validated (Post type removed)"
        );

        // user_frag references User which didn't change - should be skipped
        assert!(
            !result.contains_key(user_frag),
            "user_frag should be skipped (User type unchanged)"
        );
    }
}

#[test]
fn schema_diff_with_variable_type_reference() {
    // Operations reference types via variables. Changing a variable's input type
    // should cause that operation to be re-validated.
    let (mut host, schema_path, file_paths) = setup_host_with_fingerprints(
        "type Query { user(id: ID!): User }\ntype User { name: String }\ninput UserInput { name: String }",
        &[
            (
                "simple_query.graphql",
                "query { user(id: \"1\") { name } }",
                DocumentKind::Executable,
            ),
            (
                "input_query.graphql",
                "query CreateUser($input: UserInput!) { user(id: \"1\") { name } }",
                DocumentKind::Executable,
            ),
        ],
    );
    let simple_query = &file_paths[0];
    let input_query = &file_paths[1];

    // Change UserInput type
    host.add_file(
        &schema_path,
        "type Query { user(id: ID!): User }\ntype User { name: String }\ninput UserInput { name: String\nemail: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // input_query references UserInput via variable - should be re-validated
        assert!(
            result.contains_key(input_query),
            "input_query should be re-validated (UserInput type changed)"
        );

        // simple_query doesn't reference UserInput - should be skipped
        // (Query type didn't change in terms of fields, but its field arg type
        // didn't change since we didn't modify ID or User)
        // Actually: Query type DID change because user field's return type User didn't change,
        // but Query type itself didn't change structurally. Let's verify.
        // Query type hash: same fields {user(id: ID!): User} - unchanged.
        assert!(
            !result.contains_key(simple_query),
            "simple_query should be skipped (doesn't reference UserInput)"
        );
    }
}

#[test]
fn first_schema_change_without_fingerprints_validates_all() {
    // Without calling update_schema_fingerprints, the first schema change
    // should fall back to validating all document files (no previous baseline).
    let (mut host, schema_path, file_paths) = setup_host(
        "type Query { hero: String }\ntype User { name: String }",
        &[
            (
                "hero_query.graphql",
                "query { hero }",
                DocumentKind::Executable,
            ),
            (
                "user_frag.graphql",
                "fragment F on User { name }",
                DocumentKind::Executable,
            ),
        ],
    );
    let hero_path = &file_paths[0];
    let user_frag = &file_paths[1];

    // Change User type without having called update_schema_fingerprints
    host.add_file(
        &schema_path,
        "type Query { hero: String }\ntype User { name: String\nemail: String }",
        Language::GraphQL,
        DocumentKind::Schema,
    );

    {
        let snapshot = host.snapshot();
        let result = snapshot.diagnostics_for_change(&schema_path);

        // Both files should be re-validated (no fingerprint baseline)
        assert!(
            result.contains_key(hero_path),
            "hero_query should be re-validated (no fingerprint baseline)"
        );
        assert!(
            result.contains_key(user_frag),
            "user_frag should be re-validated (no fingerprint baseline)"
        );
    }
}
