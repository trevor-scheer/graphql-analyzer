use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use graphql_db::{
    FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles, RootDatabase,
};
use graphql_ide::AnalysisHost;
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

/// Helper to create a database with schema and document
fn setup_db_with_schema_and_doc() -> RootDatabase {
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
    let doc_content = FileContent::new(&db, Arc::from(SAMPLE_OPERATION));
    let doc_meta = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        FileKind::ExecutableGraphQL,
    );

    let project_files = ProjectFiles::new(
        &db,
        Arc::new(vec![(schema_id, schema_content, schema_meta)]),
        Arc::new(vec![(doc_id, doc_content, doc_meta)]),
    );

    db.set_project_files(Some(project_files));
    db
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
                setup_db_with_schema_and_doc()
            },
            |db| {
                // Measure: Extract schema types for first time
                let project_files = db.project_files().unwrap();
                black_box(graphql_hir::schema_types_with_project(&db, project_files))
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_schema_types_warm(c: &mut Criterion) {
    c.bench_function("schema_types_warm", |b| {
        // Setup: Extract schema types once to populate cache
        let db = setup_db_with_schema_and_doc();
        let project_files = db.project_files().unwrap();
        let _ = graphql_hir::schema_types_with_project(&db, project_files);

        b.iter(|| {
            // Measure: Should be instant (cached)
            black_box(graphql_hir::schema_types_with_project(&db, project_files))
        });
    });
}

/// Golden invariant benchmark: editing operation body doesn't invalidate schema
fn bench_golden_invariant(c: &mut Criterion) {
    c.bench_function("golden_invariant_schema_after_body_edit", |b| {
        b.iter_batched(
            || {
                // Setup: Database with schema and doc, schema types cached
                let db = setup_db_with_schema_and_doc();
                let project_files = db.project_files().unwrap();
                let _ = graphql_hir::schema_types_with_project(&db, project_files);

                // Now edit the operation body
                let doc_id = FileId::new(1);
                let new_content = FileContent::new(
                    &db,
                    Arc::from(
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
                    ),
                );
                let doc_meta = FileMetadata::new(
                    &db,
                    doc_id,
                    FileUri::new("query.graphql"),
                    FileKind::ExecutableGraphQL,
                );

                // Update project files with new content
                let schema_files = project_files.schema_files(&db);
                let new_project_files = ProjectFiles::new(
                    &db,
                    schema_files,
                    Arc::new(vec![(doc_id, new_content, doc_meta)]),
                );
                db.set_project_files(Some(new_project_files));

                (db, new_project_files)
            },
            |(db, project_files)| {
                // Measure: Schema types query should be instant (cached, not invalidated)
                black_box(graphql_hir::schema_types_with_project(&db, project_files))
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

                let project_files = ProjectFiles::new(
                    &db,
                    Arc::new(vec![(schema_id, schema_content, schema_meta)]),
                    Arc::new(vec![(doc_id, doc_content, doc_meta)]),
                );

                db.set_project_files(Some(project_files));
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

        let project_files = ProjectFiles::new(
            &db,
            Arc::new(vec![(schema_id, schema_content, schema_meta)]),
            Arc::new(vec![(doc_id, doc_content, doc_meta)]),
        );

        db.set_project_files(Some(project_files));
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

        b.iter(|| {
            // Measure: Get diagnostics (should be cached after first call)
            let snapshot = host.snapshot();
            black_box(snapshot.diagnostics(&doc_path))
        });
    });
}

criterion_group!(
    benches,
    bench_parse_cold,
    bench_parse_warm,
    bench_schema_types_cold,
    bench_schema_types_warm,
    bench_golden_invariant,
    bench_fragment_resolution_cold,
    bench_fragment_resolution_warm,
    bench_analysis_host_add_file,
    bench_analysis_host_diagnostics,
);

criterion_main!(benches);
