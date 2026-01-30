//! Shared test fixtures for GraphQL schemas and documents.
//!
//! This module provides commonly used schema and document fixtures for testing.
//! Use these for tests that don't need custom schemas. For tests where the
//! schema structure is important to the test case, prefer inline fixtures
//! to keep the test self-documenting.
//!
//! # Guidelines
//!
//! - **Use shared fixtures** when the specific schema doesn't matter, just that
//!   it's valid and has certain characteristics (types with ID fields, nested types, etc.)
//! - **Use inline fixtures** when the test is specifically about the schema structure
//!   or when seeing the schema helps understand what's being tested.

/// Minimal schema with just Query and a User type.
///
/// Good for basic validation tests where you just need a valid schema.
pub const BASIC_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
    users: [User!]!
}

type User {
    id: ID!
    name: String!
    email: String!
}
"#;

/// Schema with nested object types for testing recursive validation.
///
/// Includes User -> Post -> Comment chain, useful for testing:
/// - Nested selection set validation
/// - Fragment spread on nested types
/// - require_id_field lint on nested selections
pub const NESTED_SCHEMA: &str = r#"
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
    comments: [Comment!]!
}

type Comment {
    id: ID!
    text: String!
    author: User!
}
"#;

/// Schema with interfaces and union types.
///
/// Useful for testing:
/// - Fragment spreads on interfaces
/// - Type condition validation
/// - Union type resolution
pub const INTERFACE_SCHEMA: &str = r#"
type Query {
    node(id: ID!): Node
    search(query: String!): [SearchResult!]!
}

interface Node {
    id: ID!
}

type User implements Node {
    id: ID!
    name: String!
    email: String!
}

type Post implements Node {
    id: ID!
    title: String!
    author: User!
}

union SearchResult = User | Post
"#;

/// Schema with input types and enums.
///
/// Useful for testing:
/// - Variable type validation
/// - Input object validation
/// - Enum value validation
pub const INPUT_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
    users(filter: UserFilter, sort: SortOrder): [User!]!
}

type Mutation {
    createUser(input: CreateUserInput!): User!
    updateUser(id: ID!, input: UpdateUserInput!): User
}

type User {
    id: ID!
    name: String!
    email: String!
    status: UserStatus!
}

input CreateUserInput {
    name: String!
    email: String!
}

input UpdateUserInput {
    name: String
    email: String
    status: UserStatus
}

input UserFilter {
    nameContains: String
    status: UserStatus
}

enum UserStatus {
    ACTIVE
    INACTIVE
    PENDING
}

enum SortOrder {
    ASC
    DESC
}
"#;

/// Schema with directives.
///
/// Useful for testing directive validation and deprecated field handling.
pub const DIRECTIVE_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
    oldUser(id: ID!): User @deprecated(reason: "Use user instead")
}

type User {
    id: ID!
    name: String!
    email: String!
    username: String @deprecated(reason: "Use name instead")
}
"#;

/// Schema without ID fields on some types.
///
/// Useful for testing the require_id_field lint rule.
pub const SCHEMA_WITHOUT_IDS: &str = r#"
type Query {
    user(id: ID!): User
    stats: Stats
}

type User {
    id: ID!
    name: String!
    stats: Stats
}

type Stats {
    viewCount: Int!
    likeCount: Int!
}
"#;

/// Simple valid query for basic tests.
pub const BASIC_QUERY: &str = r#"
query GetUser($id: ID!) {
    user(id: $id) {
        id
        name
        email
    }
}
"#;

/// Query with fragment spread.
pub const QUERY_WITH_FRAGMENT: &str = r#"
query GetUser($id: ID!) {
    user(id: $id) {
        ...UserFields
    }
}

fragment UserFields on User {
    id
    name
    email
}
"#;

/// Fragment definition only (no operations).
pub const FRAGMENT_ONLY: &str = r#"
fragment UserFields on User {
    id
    name
    email
}
"#;

/// Multiple fragments for testing cross-file fragment resolution.
pub const MULTIPLE_FRAGMENTS: &str = r#"
fragment UserBasic on User {
    id
    name
}

fragment UserDetails on User {
    ...UserBasic
    email
}
"#;
