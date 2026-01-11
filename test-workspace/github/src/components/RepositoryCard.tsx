// TypeScript file with embedded GraphQL
// Tests fragment-only TypeScript file

import { gql } from '@apollo/client';

// Multiple fragments in one template literal - edge case
export const REPO_CARD_FRAGMENTS = gql`
  fragment RepoCardBasic on Repository {
    id
    name
    nameWithOwner
    description
    url
    isPrivate
    isFork
    isArchived
  }

  fragment RepoCardStats on Repository {
    stargazerCount
    forkCount
    watchers {
      totalCount
    }
    issues(states: [OPEN]) {
      totalCount
    }
    pullRequests(states: [OPEN]) {
      totalCount
    }
  }

  fragment RepoCardLanguage on Repository {
    primaryLanguage {
      name
      color
    }
    languages(first: 3, orderBy: { field: SIZE, direction: DESC }) {
      nodes {
        name
        color
      }
    }
  }
`;

// Separate template for the full card
export const REPO_CARD_FULL = gql`
  fragment RepoCardFull on Repository {
    ...RepoCardBasic
    ...RepoCardStats
    ...RepoCardLanguage
    owner {
      login
      avatarUrl
    }
    defaultBranchRef {
      name
    }
    pushedAt
    updatedAt
  }
`;

// Query that uses the fragments
export const GET_REPO_CARD = gql`
  query GetRepoCard($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      ...RepoCardFull
    }
  }
`;

// List query
export const GET_REPO_CARDS = gql`
  query GetRepoCards($login: String!, $first: Int = 20, $after: String) {
    user(login: $login) {
      repositories(first: $first, after: $after, orderBy: { field: UPDATED_AT, direction: DESC }) {
        pageInfo {
          hasNextPage
          endCursor
        }
        nodes {
          ...RepoCardFull
        }
      }
    }
  }
`;

interface RepositoryCardProps {
  owner: string;
  name: string;
}

export function RepositoryCard({ owner, name }: RepositoryCardProps) {
  return null;
}
