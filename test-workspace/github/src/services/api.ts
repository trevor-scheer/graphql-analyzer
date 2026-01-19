// Service file with GraphQL operations
// Tests extraction from service/utility files

import { gql } from "@apollo/client";

// Rate limit query - useful for API management
export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      remaining
      resetAt
      used
      nodeCount
    }
  }
`;

// Node lookup - for resolving IDs
export const NODE_QUERY = gql`
  query GetNode($id: ID!) {
    node(id: $id) {
      id
      ... on Repository {
        ...RepositoryBasic
      }
      ... on Issue {
        ...IssueBasic
      }
      ... on PullRequest {
        ...PullRequestBasic
      }
      ... on User {
        ...UserBasic
      }
      ... on Organization {
        ...OrganizationBasic
      }
    }
  }
`;

// Batch node lookup
export const NODES_QUERY = gql`
  query GetNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      id
      ... on Repository {
        nameWithOwner
        url
      }
      ... on Issue {
        number
        title
        url
      }
      ... on PullRequest {
        number
        title
        url
      }
      ... on User {
        login
        url
      }
    }
  }
`;

// Resource lookup by URL
export const RESOURCE_QUERY = gql`
  query GetResource($url: URI!) {
    resource(url: $url) {
      ... on Repository {
        ...RepositoryDetails
      }
      ... on Issue {
        ...IssueDetails
      }
      ... on PullRequest {
        ...PullRequestDetails
      }
      ... on User {
        ...UserProfile
      }
      ... on Organization {
        ...OrganizationDetails
      }
    }
  }
`;

// Viewer permission check
export const VIEWER_PERMISSIONS = gql`
  query GetViewerPermissions($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      id
      viewerPermission
      viewerCanAdminister
      viewerCanCreateProjects
      viewerCanSubscribe
      viewerCanUpdateTopics
      viewerHasStarred
      viewerSubscription
    }
  }
`;

// API service class would use these queries
export class GitHubApiService {
  async checkRateLimit() {
    // Implementation
  }

  async resolveNode(id: string) {
    // Implementation
  }

  async resolveUrl(url: string) {
    // Implementation
  }
}
