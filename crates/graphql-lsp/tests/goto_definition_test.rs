use graphql_project::{DocumentIndex, FragmentInfo, GotoDefinitionProvider, Position, SchemaIndex};

#[test]
fn test_goto_definition_integration() {
    // Set up document index with a fragment
    let mut doc_index = DocumentIndex::new();
    doc_index.add_fragment(
        "UserFields".to_string(),
        FragmentInfo {
            name: "UserFields".to_string(),
            type_condition: "User".to_string(),
            file_path: "/test/fragments.graphql".to_string(),
            line: 0,
            column: 9, // "fragment UserFields" - the U is at column 9
        },
    );

    let schema_index = SchemaIndex::new();
    let provider = GotoDefinitionProvider::new();

    // Document with a fragment spread
    let document = r"
query GetUser {
    user {
        ...UserFields
    }
}
";

    // Position on "UserFields" in the spread (line 3, column 11)
    // Line 3 is "        ...UserFields"
    // Column 11 is the "U" in "UserFields"
    let position = Position {
        line: 3,
        character: 11,
    };

    let locations = provider
        .goto_definition(document, position, &doc_index, &schema_index)
        .expect("Should find definition");

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].file_path, "/test/fragments.graphql");
    assert_eq!(locations[0].range.start.line, 0);
    assert_eq!(locations[0].range.start.character, 9);
}
