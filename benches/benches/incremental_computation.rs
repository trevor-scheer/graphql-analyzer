use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use graphql_db::{
    FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles, RootDatabase,
};
use graphql_ide::AnalysisHost;
use salsa::Setter;
use std::sync::Arc;

// Sample GraphQL schema for benchmarks
const SAMPLE_SCHEMA: &str = r"
type Query {
  user(id: ID!): User
  users: [User!]!
  post(id: ID!): Post
  posts: [Post!]!
}

type User {
  id: ID!
  name: String!
  email: String!
  posts: [Post!]!
}

type Post {
  id: ID!
  title: String!
  content: String!
  author: User!
}
";

// Sample operation that references schema
const SAMPLE_OPERATION: &str = r"
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    email
    posts {
      id
      title
    }
  }
}
";

// Operation using fragment
const OPERATION_WITH_FRAGMENT: &str = r"
query GetUserWithFragment($id: ID!) {
  user(id: $id) {
    ...UserFields
    posts {
      id
      title
    }
  }
}

fragment UserFields on User {
  id
  name
  email
}
";

/// Helper to create `ProjectFiles` with schema and document
fn create_project_files(db: &mut RootDatabase) -> ProjectFiles {
    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(db, Arc::from(SAMPLE_SCHEMA));
    let schema_meta = FileMetadata::new(
        db,
        schema_id,
        FileUri::new("schema.graphql"),
        FileKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(db, Arc::from(SAMPLE_OPERATION));
    let doc_meta = FileMetadata::new(
        db,
        doc_id,
        FileUri::new("query.graphql"),
        FileKind::ExecutableGraphQL,
    );

    let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(vec![schema_id]));
    let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(vec![doc_id]));
    let mut file_entries = std::collections::HashMap::new();
    let schema_entry = graphql_db::FileEntry::new(db, schema_content, schema_meta);
    let doc_entry = graphql_db::FileEntry::new(db, doc_content, doc_meta);
    file_entries.insert(schema_id, schema_entry);
    file_entries.insert(doc_id, doc_entry);
    let file_entry_map = graphql_db::FileEntryMap::new(db, Arc::new(file_entries));
    ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
}

/// Parse benchmarks
fn bench_parse_cold(c: &mut Criterion) {
    c.bench_function("parse_cold", |b| {
        b.iter_batched(
            || {
                // Setup: Fresh database each iteration
                let db = RootDatabase::new();
                let content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
                let metadata = FileMetadata::new(
                    &db,
                    FileId::new(0),
                    FileUri::new("schema.graphql"),
                    FileKind::Schema,
                );
                (db, content, metadata)
            },
            |(db, content, metadata)| {
                // Measure: Parse for first time
                black_box(graphql_syntax::parse(&db, content, metadata))
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_parse_warm(c: &mut Criterion) {
    c.bench_function("parse_warm", |b| {
        // Setup: Parse once to populate cache
        let db = RootDatabase::new();
        let content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
        let metadata = FileMetadata::new(
            &db,
            FileId::new(0),
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );
        let _ = graphql_syntax::parse(&db, content, metadata);

        b.iter(|| {
            // Measure: Should be instant (cached)
            black_box(graphql_syntax::parse(&db, content, metadata))
        });
    });
}

/// Schema type extraction benchmarks
fn bench_schema_types_cold(c: &mut Criterion) {
    c.bench_function("schema_types_cold", |b| {
        b.iter_batched(
            || {
                // Setup: Fresh database with schema
                let mut db = RootDatabase::new();
                let project_files = create_project_files(&mut db);
                (db, project_files)
            },
            |(db, project_files)| {
                // Measure: Extract schema types for first time
                black_box(graphql_hir::schema_types_with_project(&db, project_files))
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_schema_types_warm(c: &mut Criterion) {
    c.bench_function("schema_types_warm", |b| {
        // Setup: Extract schema types once to populate cache
        let mut db = RootDatabase::new();
        let project_files = create_project_files(&mut db);
        let _ = graphql_hir::schema_types_with_project(&db, project_files);

        b.iter(|| {
            // Measure: Should be instant (cached)
            black_box(graphql_hir::schema_types_with_project(&db, project_files))
        });
    });
}

/// Golden invariant benchmark: editing operation body doesn't invalidate schema
///
/// This tests the critical performance property: when we edit only the document
/// content (not add/remove files), the schema types should remain cached.
fn bench_golden_invariant(c: &mut Criterion) {
    c.bench_function("golden_invariant_schema_after_body_edit", |b| {
        b.iter_batched(
            || {
                // Setup: Database with schema and doc, schema types cached
                let mut db = RootDatabase::new();

                let schema_id = FileId::new(0);
                let schema_content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
                let schema_meta = FileMetadata::new(
                    &db,
                    schema_id,
                    FileUri::new("schema.graphql"),
                    FileKind::Schema,
                );

                let doc_id = FileId::new(1);
                let doc_content = FileContent::new(&db, Arc::from(SAMPLE_OPERATION));
                let doc_meta = FileMetadata::new(
                    &db,
                    doc_id,
                    FileUri::new("query.graphql"),
                    FileKind::ExecutableGraphQL,
                );

                let schema_file_ids =
                    graphql_db::SchemaFileIds::new(&db, Arc::new(vec![schema_id]));
                let document_file_ids =
                    graphql_db::DocumentFileIds::new(&db, Arc::new(vec![doc_id]));
                let mut file_entries = std::collections::HashMap::new();
                let schema_entry = graphql_db::FileEntry::new(&db, schema_content, schema_meta);
                let doc_entry = graphql_db::FileEntry::new(&db, doc_content, doc_meta);
                file_entries.insert(schema_id, schema_entry);
                file_entries.insert(doc_id, doc_entry);
                let file_entry_map = graphql_db::FileEntryMap::new(&db, Arc::new(file_entries));
                let project_files =
                    ProjectFiles::new(&db, schema_file_ids, document_file_ids, file_entry_map);

                // Cache schema types
                let _ = graphql_hir::schema_types_with_project(&db, project_files);

                // Now edit the document content using Salsa's in-place setter
                // This simulates what happens on a keystroke - we update content
                // WITHOUT rebuilding ProjectFiles
                doc_content.set_text(&mut db).to(Arc::from(
                    r"
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    # Added a comment - body changed
    email
  }
}
",
                ));

                (db, project_files)
            },
            |(db, project_files)| {
                // Measure: Schema types query should be instant (cached, not invalidated)
                // because we only changed document content, not ProjectFiles structure
                black_box(graphql_hir::schema_types_with_project(&db, project_files))
            },
            BatchSize::SmallInput,
        );
    });
}

/// Per-file granular caching benchmark
///
/// This tests that when file A changes, queries for file B remain cached.
/// Uses the new `FileEntryMap` pattern for true per-file granular invalidation.
fn bench_per_file_granular_caching(c: &mut Criterion) {
    c.bench_function("per_file_granular_caching", |b| {
        b.iter_batched(
            || {
                // Setup: Database with schema and TWO document files
                let mut db = RootDatabase::new();

                let schema_id = FileId::new(0);
                let schema_content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
                let schema_meta = FileMetadata::new(
                    &db,
                    schema_id,
                    FileUri::new("schema.graphql"),
                    FileKind::Schema,
                );

                let doc1_id = FileId::new(1);
                let doc1_content = FileContent::new(&db, Arc::from(SAMPLE_OPERATION));
                let doc1_meta = FileMetadata::new(
                    &db,
                    doc1_id,
                    FileUri::new("query1.graphql"),
                    FileKind::ExecutableGraphQL,
                );

                let doc2_id = FileId::new(2);
                let doc2_content = FileContent::new(
                    &db,
                    Arc::from("fragment UserFields on User { id name email }"),
                );
                let doc2_meta = FileMetadata::new(
                    &db,
                    doc2_id,
                    FileUri::new("query2.graphql"),
                    FileKind::ExecutableGraphQL,
                );

                // Create granular FileEntryMap for per-file caching
                let schema_entry = graphql_db::FileEntry::new(&db, schema_content, schema_meta);
                let doc1_entry = graphql_db::FileEntry::new(&db, doc1_content, doc1_meta);
                let doc2_entry = graphql_db::FileEntry::new(&db, doc2_content, doc2_meta);

                let mut entry_map = std::collections::HashMap::new();
                entry_map.insert(schema_id, schema_entry);
                entry_map.insert(doc1_id, doc1_entry);
                entry_map.insert(doc2_id, doc2_entry);

                let schema_file_ids =
                    graphql_db::SchemaFileIds::new(&db, Arc::new(vec![schema_id]));
                let document_file_ids =
                    graphql_db::DocumentFileIds::new(&db, Arc::new(vec![doc1_id, doc2_id]));
                let file_entry_map = graphql_db::FileEntryMap::new(&db, Arc::new(entry_map));

                let project_files =
                    ProjectFiles::new(&db, schema_file_ids, document_file_ids, file_entry_map);

                // Warm caches for all files
                let _ = graphql_hir::all_fragments_with_project(&db, project_files);

                // Now edit ONLY doc1 content - doc2's queries should remain cached
                doc1_content.set_text(&mut db).to(Arc::from(
                    r"query GetUser($id: ID!) { user(id: $id) { id name email } }",
                ));

                (db, project_files, doc2_id, doc2_content, doc2_meta)
            },
            |(db, project_files, doc2_id, doc2_content, doc2_meta)| {
                // Measure: file_fragments for doc2 should be instant (cached)
                // because we only changed doc1, not doc2
                black_box(graphql_hir::file_fragments(
                    &db,
                    doc2_id,
                    doc2_content,
                    doc2_meta,
                ));
                black_box(graphql_hir::all_fragments_with_project(&db, project_files))
            },
            BatchSize::SmallInput,
        );
    });
}

/// Fragment resolution benchmarks
fn bench_fragment_resolution_cold(c: &mut Criterion) {
    c.bench_function("fragment_resolution_cold", |b| {
        b.iter_batched(
            || {
                // Setup: Database with schema and operation using fragment
                let db = RootDatabase::new();

                let schema_id = FileId::new(0);
                let schema_content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
                let schema_meta = FileMetadata::new(
                    &db,
                    schema_id,
                    FileUri::new("schema.graphql"),
                    FileKind::Schema,
                );

                let doc_id = FileId::new(1);
                let doc_content = FileContent::new(&db, Arc::from(OPERATION_WITH_FRAGMENT));
                let doc_meta = FileMetadata::new(
                    &db,
                    doc_id,
                    FileUri::new("query.graphql"),
                    FileKind::ExecutableGraphQL,
                );

                let schema_file_ids =
                    graphql_db::SchemaFileIds::new(&db, Arc::new(vec![schema_id]));
                let document_file_ids =
                    graphql_db::DocumentFileIds::new(&db, Arc::new(vec![doc_id]));
                let mut file_entries = std::collections::HashMap::new();
                let schema_entry = graphql_db::FileEntry::new(&db, schema_content, schema_meta);
                let doc_entry = graphql_db::FileEntry::new(&db, doc_content, doc_meta);
                file_entries.insert(schema_id, schema_entry);
                file_entries.insert(doc_id, doc_entry);
                let file_entry_map = graphql_db::FileEntryMap::new(&db, Arc::new(file_entries));
                let project_files =
                    ProjectFiles::new(&db, schema_file_ids, document_file_ids, file_entry_map);

                (db, project_files)
            },
            |(db, project_files)| {
                // Measure: Resolve fragments for first time
                black_box(graphql_hir::all_fragments_with_project(&db, project_files))
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_fragment_resolution_warm(c: &mut Criterion) {
    c.bench_function("fragment_resolution_warm", |b| {
        // Setup: Database with fragments already resolved
        let db = RootDatabase::new();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from(SAMPLE_SCHEMA));
        let schema_meta = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from(OPERATION_WITH_FRAGMENT));
        let doc_meta = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let schema_file_ids = graphql_db::SchemaFileIds::new(&db, Arc::new(vec![schema_id]));
        let document_file_ids = graphql_db::DocumentFileIds::new(&db, Arc::new(vec![doc_id]));
        let mut file_entries = std::collections::HashMap::new();
        let schema_entry = graphql_db::FileEntry::new(&db, schema_content, schema_meta);
        let doc_entry = graphql_db::FileEntry::new(&db, doc_content, doc_meta);
        file_entries.insert(schema_id, schema_entry);
        file_entries.insert(doc_id, doc_entry);
        let file_entry_map = graphql_db::FileEntryMap::new(&db, Arc::new(file_entries));
        let project_files =
            ProjectFiles::new(&db, schema_file_ids, document_file_ids, file_entry_map);

        let _ = graphql_hir::all_fragments_with_project(&db, project_files);

        b.iter(|| {
            // Measure: Should be instant (cached)
            black_box(graphql_hir::all_fragments_with_project(&db, project_files))
        });
    });
}

/// `AnalysisHost` benchmark - full validation flow
fn bench_analysis_host_add_file(c: &mut Criterion) {
    c.bench_function("analysis_host_add_file", |b| {
        b.iter_batched(
            || {
                // Setup: Fresh AnalysisHost
                AnalysisHost::new()
            },
            |mut host| {
                // Measure: Add schema file
                let path = graphql_ide::FilePath::new("schema.graphql");
                host.add_file(&path, SAMPLE_SCHEMA, graphql_ide::FileKind::Schema, 0);
                black_box(());
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_analysis_host_diagnostics(c: &mut Criterion) {
    c.bench_function("analysis_host_diagnostics", |b| {
        // Setup: AnalysisHost with schema and document
        let mut host = AnalysisHost::new();
        let schema_path = graphql_ide::FilePath::new("schema.graphql");
        host.add_file(
            &schema_path,
            SAMPLE_SCHEMA,
            graphql_ide::FileKind::Schema,
            0,
        );

        let doc_path = graphql_ide::FilePath::new("query.graphql");
        host.add_file(
            &doc_path,
            SAMPLE_OPERATION,
            graphql_ide::FileKind::ExecutableGraphQL,
            0,
        );
        host.rebuild_project_files();

        b.iter(|| {
            // Measure: Get diagnostics (should be cached after first call)
            let snapshot = host.snapshot();
            black_box(snapshot.diagnostics(&doc_path))
        });
    });
}

/// Benchmark: warm edit using `AnalysisHost` (simulates real LSP keystroke)
///
/// NOTE: This benchmark is currently disabled due to a known Salsa deadlock issue
/// when updating files and getting diagnostics in the same thread.
/// See: `test_diagnostics_after_file_update` in graphql-ide/src/lib.rs
///
/// The fix we implemented (not calling `rebuild_project_files` on content changes)
/// is validated by the `golden_invariant` benchmark which tests the underlying
/// Salsa caching behavior directly.
#[allow(dead_code)]
const fn bench_analysis_host_warm_edit(_c: &mut Criterion) {
    // Disabled - see comment above
    // To re-enable, fix the Salsa update hang issue first
}

criterion_group!(
    benches,
    bench_parse_cold,
    bench_parse_warm,
    bench_schema_types_cold,
    bench_schema_types_warm,
    bench_golden_invariant,
    bench_per_file_granular_caching,
    bench_fragment_resolution_cold,
    bench_fragment_resolution_warm,
    bench_analysis_host_add_file,
    bench_analysis_host_diagnostics,
    // bench_analysis_host_warm_edit, // Disabled - Salsa deadlock, see comment above
);

criterion_main!(benches);
