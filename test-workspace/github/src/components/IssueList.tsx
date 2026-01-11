// TypeScript file with embedded GraphQL
// Tests extraction from template literals

import { gql } from '@apollo/client';

// Single template literal with query
const GET_ISSUES = gql`
  query GetIssuesList($owner: String!, $name: String!, $first: Int = 30, $after: String, $states: [IssueState!]) {
    repository(owner: $owner, name: $name) {
      issues(first: $first, after: $after, states: $states, orderBy: { field: CREATED_AT, direction: DESC }) {
        totalCount
        pageInfo {
          ...PageInfoFull
        }
        nodes {
          ...IssueWithLabels
        }
      }
    }
  }
`;

// Fragment defined in TypeScript
const ISSUE_ROW_FRAGMENT = gql`
  fragment IssueRow on Issue {
    id
    number
    title
    state
    createdAt
    author {
      login
      avatarUrl
    }
    labels(first: 5) {
      nodes {
        name
        color
      }
    }
    comments {
      totalCount
    }
  }
`;

// Multiple queries in one file
const GET_ISSUE_DETAIL = gql`
  query GetIssueDetail($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      issue(number: $number) {
        ...IssueFull
        timelineItems(first: 50) {
          nodes {
            ... on IssueComment {
              id
              body
              createdAt
              author {
                ...ActorBasic
              }
            }
          }
        }
      }
    }
  }
`;

// Mutation
const CREATE_ISSUE_MUTATION = gql`
  mutation CreateNewIssue($input: CreateIssueInput!) {
    createIssue(input: $input) {
      issue {
        ...IssueRow
      }
    }
  }
`;

interface IssueListProps {
  owner: string;
  name: string;
}

export function IssueList({ owner, name }: IssueListProps) {
  // Component implementation would go here
  return null;
}

export { GET_ISSUES, ISSUE_ROW_FRAGMENT, GET_ISSUE_DETAIL, CREATE_ISSUE_MUTATION };
