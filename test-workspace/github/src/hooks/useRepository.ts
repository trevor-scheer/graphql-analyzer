// TypeScript hooks file with embedded GraphQL
// Tests extraction from .ts files (not .tsx)

import { gql, useQuery, useMutation } from "@apollo/client";

// Query for repository data
const REPOSITORY_QUERY = gql`
  query UseRepository($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      ...RepositoryDetails
      ...RepositoryTopics
      viewerHasStarred
      viewerSubscription
      viewerPermission
      viewerCanAdminister
    }
  }
`;

// Star/unstar mutations
const STAR_MUTATION = gql`
  mutation StarRepository($id: ID!) {
    addStar(input: { starrableId: $id }) {
      starrable {
        ... on Repository {
          id
          stargazerCount
          viewerHasStarred
        }
      }
    }
  }
`;

const UNSTAR_MUTATION = gql`
  mutation UnstarRepository($id: ID!) {
    removeStar(input: { starrableId: $id }) {
      starrable {
        ... on Repository {
          id
          stargazerCount
          viewerHasStarred
        }
      }
    }
  }
`;

// Watch mutations
const WATCH_MUTATION = gql`
  mutation WatchRepository($id: ID!, $state: SubscriptionState!) {
    updateSubscription(input: { subscribableId: $id, state: $state }) {
      subscribable {
        ... on Repository {
          id
          viewerSubscription
        }
      }
    }
  }
`;

interface UseRepositoryOptions {
  owner: string;
  name: string;
}

export function useRepository({ owner, name }: UseRepositoryOptions) {
  // Hook implementation would go here
  return {
    data: null,
    loading: false,
    error: null,
    star: () => {},
    unstar: () => {},
    watch: () => {},
  };
}

export { REPOSITORY_QUERY, STAR_MUTATION, UNSTAR_MUTATION, WATCH_MUTATION };
