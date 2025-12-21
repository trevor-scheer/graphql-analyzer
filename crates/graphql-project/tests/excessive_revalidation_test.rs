/// Test to demonstrate the excessive revalidation issue
///
/// This test simulates the scenario where changing a query in one file
/// causes unnecessary revalidation of unrelated fragment files.
use graphql_config::{DocumentsConfig, ProjectConfig, SchemaConfig};
use graphql_project::{DependencyGraph, DynamicGraphQLProject};
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

/// Create a test workspace with:
/// - A schema
/// - 3 fragment files (`FragA`, `FragB`, `FragC`)
/// - 2 query files (`Query1` uses `FragA`, `Query2` is standalone)
fn create_test_workspace() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path();

    // Schema
    let schema = r"
type Query {
    user(id: ID!): User
    posts: [Post!]!
}

type User {
    id: ID!
    name: String!
    email: String
}

type Post {
    id: ID!
    title: String!
    content: String!
}
";
    fs::write(base_path.join("schema.graphql"), schema).unwrap();

    // Fragment A - used by Query1
    let fragment_a = r"
fragment UserFields on User {
    id
    name
    email
}
";
    fs::write(base_path.join("fragment_a.graphql"), fragment_a).unwrap();

    // Fragment B - not used by anyone
    let fragment_b = r"
fragment PostFields on Post {
    id
    title
}
";
    fs::write(base_path.join("fragment_b.graphql"), fragment_b).unwrap();

    // Fragment C - not used by anyone
    let fragment_c = r"
fragment ExtendedPostFields on Post {
    id
    title
    content
}
";
    fs::write(base_path.join("fragment_c.graphql"), fragment_c).unwrap();

    // Query1 - uses FragA
    let query1 = r"
query GetUser($id: ID!) {
    user(id: $id) {
        ...UserFields
    }
}
";
    fs::write(base_path.join("query1.graphql"), query1).unwrap();

    // Query2 - standalone, doesn't use any fragments
    let query2 = r"
query GetPosts {
    posts {
        id
        title
    }
}
";
    fs::write(base_path.join("query2.graphql"), query2).unwrap();

    temp_dir
}

#[tokio::test]
async fn test_changing_query_should_not_revalidate_unrelated_fragments() {
    let workspace = create_test_workspace();

    // Create project config
    let config = ProjectConfig {
        schema: SchemaConfig::Path("schema.graphql".to_string()),
        documents: Some(DocumentsConfig::Pattern("*.graphql".to_string())),
        include: None,
        exclude: None,
        lint: None,
        extensions: None,
    };

    // Initialize project
    let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
    project.initialize().await.expect("Failed to initialize");

    // Build dependency graph
    let dep_graph = {
        let document_index_lock = project.document_index();
        let document_index = document_index_lock.read().unwrap();
        let schema_index_lock = project.schema_index();
        let schema_index = schema_index_lock.read().unwrap();
        DependencyGraph::build(&schema_index, &document_index)
    };

    // Verify the graph captured the fragments
    assert_eq!(
        dep_graph.fragment_count(),
        3,
        "Should have 3 fragments indexed"
    );

    // Simulate changing Query2 (which doesn't use any fragments)
    let changed_file = workspace.path().join("query2.graphql");

    // Get affected files
    let affected_files = dep_graph.get_affected_files(&changed_file);

    println!("\nChanged file: {changed_file:?}");
    println!("Affected files: {affected_files:#?}");

    // Expected behavior: Only query2.graphql should be affected
    // Current broken behavior: The LSP would revalidate ALL fragment files
    assert_eq!(
        affected_files.len(),
        1,
        "Only the changed file should be affected, but got {} affected files",
        affected_files.len()
    );
    assert!(
        affected_files.contains(&changed_file),
        "Affected files should include the changed file"
    );

    // These fragment files should NOT be in the affected set
    let fragment_b_path = workspace.path().join("fragment_b.graphql");
    let fragment_c_path = workspace.path().join("fragment_c.graphql");

    assert!(
        !affected_files.contains(&fragment_b_path),
        "FragmentB should NOT be affected by Query2 change"
    );
    assert!(
        !affected_files.contains(&fragment_c_path),
        "FragmentC should NOT be affected by Query2 change"
    );

    // Even FragmentA (which is used by Query1) should not be affected
    // because we changed Query2, not Query1
    let fragment_a_path = workspace.path().join("fragment_a.graphql");
    assert!(
        !affected_files.contains(&fragment_a_path),
        "FragmentA should NOT be affected by Query2 change"
    );
}

#[tokio::test]
async fn test_changing_operation_should_only_affect_used_fragments() {
    let workspace = create_test_workspace();

    let config = ProjectConfig {
        schema: SchemaConfig::Path("schema.graphql".to_string()),
        documents: Some(DocumentsConfig::Pattern("*.graphql".to_string())),
        include: None,
        exclude: None,
        lint: None,
        extensions: None,
    };

    let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
    project.initialize().await.expect("Failed to initialize");

    let dep_graph = {
        let document_index_lock = project.document_index();
        let document_index = document_index_lock.read().unwrap();
        let schema_index_lock = project.schema_index();
        let schema_index = schema_index_lock.read().unwrap();
        DependencyGraph::build(&schema_index, &document_index)
    };

    // Simulate changing Query1 (which uses FragmentA)
    let changed_file = workspace.path().join("query1.graphql");

    let affected_files = dep_graph.get_affected_files(&changed_file);

    println!("\nChanged file: {changed_file:?}");
    println!("Affected files: {affected_files:#?}");

    // Expected: Only query1.graphql should be affected
    // Note: Currently the dependency graph doesn't track fragment usage by operations,
    // so it won't mark FragmentA as affected. However, the issue is that the LSP
    // revalidates ALL fragments regardless of the dependency graph.
    assert_eq!(
        affected_files.len(),
        1,
        "Only the changed file should be affected when we change an operation"
    );

    // The unrelated fragments definitely should NOT be affected
    let fragment_b_path = workspace.path().join("fragment_b.graphql");
    let fragment_c_path = workspace.path().join("fragment_c.graphql");

    assert!(
        !affected_files.contains(&fragment_b_path),
        "FragmentB should NOT be affected by Query1 change"
    );
    assert!(
        !affected_files.contains(&fragment_c_path),
        "FragmentC should NOT be affected by Query1 change"
    );
}

#[tokio::test]
async fn test_current_lsp_behavior_revalidates_all_fragments() {
    let workspace = create_test_workspace();

    let config = ProjectConfig {
        schema: SchemaConfig::Path("schema.graphql".to_string()),
        documents: Some(DocumentsConfig::Pattern("*.graphql".to_string())),
        include: None,
        exclude: None,
        lint: None,
        extensions: None,
    };

    let mut project = DynamicGraphQLProject::new(config, Some(workspace.path().to_path_buf()));
    project.initialize().await.expect("Failed to initialize");

    // Simulate what the LSP currently does in revalidate_fragment_files:
    // It gets ALL fragment files from the document index
    let all_fragment_files: HashSet<String> = {
        let document_index_lock = project.document_index();
        let document_index = document_index_lock.read().unwrap();
        document_index
            .fragments
            .values()
            .flatten()
            .map(|frag_info| frag_info.file_path.clone())
            .collect()
    };

    println!("\nCurrent LSP behavior - ALL fragment files to revalidate:");
    for file in &all_fragment_files {
        println!("  - {file}");
    }

    // This demonstrates the problem: the LSP revalidates ALL 3 fragments
    // regardless of which file actually changed
    assert_eq!(
        all_fragment_files.len(),
        3,
        "Current behavior revalidates ALL {} fragment files on EVERY document change",
        all_fragment_files.len()
    );

    // In a large project like Obsidian, this could be hundreds of fragment files!
}
