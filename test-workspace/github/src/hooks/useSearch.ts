// TypeScript hooks for search functionality
// Tests multiple queries in a hook file

import { gql, useLazyQuery } from "@apollo/client";

// Search queries for different types
const SEARCH_REPOS = gql`
  query SearchRepos($query: String!, $first: Int = 20, $after: String) {
    search(query: $query, type: REPOSITORY, first: $first, after: $after) {
      repositoryCount
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        ... on Repository {
          ...RepositoryDetails
          ...RepositoryLanguages
        }
      }
    }
  }
`;

const SEARCH_ISSUES = gql`
  query SearchIssues($query: String!, $first: Int = 20, $after: String) {
    search(query: $query, type: ISSUE, first: $first, after: $after) {
      issueCount
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        ... on Issue {
          ...IssueWithLabels
          repository {
            nameWithOwner
            url
          }
        }
        ... on PullRequest {
          ...PullRequestDetails
          repository {
            nameWithOwner
            url
          }
        }
      }
    }
  }
`;

const SEARCH_USERS = gql`
  query SearchUsers($query: String!, $first: Int = 20, $after: String) {
    search(query: $query, type: USER, first: $first, after: $after) {
      userCount
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        ... on User {
          ...UserProfile
          repositories {
            totalCount
          }
        }
        ... on Organization {
          ...OrganizationDetails
          repositories {
            totalCount
          }
        }
      }
    }
  }
`;

// Combined search fragment for autocomplete
const SEARCH_AUTOCOMPLETE = gql`
  fragment SearchAutocompleteResult on SearchResultItem {
    ... on Repository {
      id
      nameWithOwner
      description
      stargazerCount
    }
    ... on Issue {
      id
      number
      title
      state
      repository {
        nameWithOwner
      }
    }
    ... on PullRequest {
      id
      number
      title
      state
      repository {
        nameWithOwner
      }
    }
    ... on User {
      id
      login
      name
      avatarUrl
    }
    ... on Organization {
      id
      login
      name
      avatarUrl
    }
  }
`;

const AUTOCOMPLETE_QUERY = gql`
  query Autocomplete($query: String!) {
    search(query: $query, type: REPOSITORY, first: 5) {
      nodes {
        ...SearchAutocompleteResult
      }
    }
  }
`;

type SearchType = "repos" | "issues" | "users";

export function useSearch(type: SearchType) {
  // Hook implementation
  return {
    search: () => {},
    data: null,
    loading: false,
  };
}

export function useAutocomplete() {
  return {
    search: () => {},
    results: [],
  };
}

export { SEARCH_REPOS, SEARCH_ISSUES, SEARCH_USERS, AUTOCOMPLETE_QUERY };
