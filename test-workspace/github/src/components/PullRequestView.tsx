// TypeScript file with embedded GraphQL for PR views
// Tests various template literal patterns

import { gql, useQuery, useMutation } from '@apollo/client';

// Using graphql-tag style
export const PR_TIMELINE_QUERY = gql`
  query PRTimeline($owner: String!, $name: String!, $number: Int!, $first: Int = 50, $after: String) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $number) {
        ...PullRequestDetails
        timelineItems(first: $first, after: $after) {
          pageInfo {
            ...PageInfoFull
          }
          nodes {
            ... on PullRequestCommit {
              commit {
                ...CommitBasic
              }
            }
            ... on PullRequestReview {
              id
              state
              body
              author {
                ...ActorBasic
              }
              submittedAt
            }
            ... on IssueComment {
              ...IssueCommentBasic
            }
            ... on MergedEvent {
              id
              actor {
                ...ActorBasic
              }
              createdAt
              mergeRefName
            }
          }
        }
      }
    }
  }
`;

// Fragment colocated with component
export const PR_REVIEW_FRAGMENT = gql`
  fragment PRReviewItem on PullRequestReview {
    id
    state
    body
    bodyHTML
    createdAt
    author {
      login
      avatarUrl
      url
    }
    comments(first: 20) {
      totalCount
      nodes {
        id
        body
        path
        line
        outdated
      }
    }
  }
`;

// Query using the colocated fragment
export const GET_PR_REVIEWS = gql`
  query GetPRReviews($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $number) {
        id
        reviews(first: 50) {
          nodes {
            ...PRReviewItem
          }
        }
        reviewDecision
        reviewRequests(first: 10) {
          nodes {
            requestedReviewer {
              ... on User {
                login
                avatarUrl
              }
              ... on Team {
                name
                slug
              }
            }
          }
        }
      }
    }
  }
`;

// Mutation for adding review
export const ADD_REVIEW_MUTATION = gql`
  mutation AddPRReview(
    $pullRequestId: ID!
    $body: String
    $event: PullRequestReviewEvent!
    $comments: [DraftPullRequestReviewComment!]
  ) {
    addPullRequestReview(
      input: {
        pullRequestId: $pullRequestId
        body: $body
        event: $event
        comments: $comments
      }
    ) {
      pullRequestReview {
        ...PRReviewItem
      }
    }
  }
`;

// Merge mutation
export const MERGE_PR_MUTATION = gql`
  mutation MergePR($pullRequestId: ID!, $mergeMethod: PullRequestMergeMethod!) {
    mergePullRequest(input: { pullRequestId: $pullRequestId, mergeMethod: $mergeMethod }) {
      pullRequest {
        id
        merged
        mergedAt
        state
      }
    }
  }
`;

interface PullRequestViewProps {
  owner: string;
  name: string;
  number: number;
}

export function PullRequestView({ owner, name, number }: PullRequestViewProps) {
  // Component implementation
  return null;
}
