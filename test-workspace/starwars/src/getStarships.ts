// Example TypeScript file with embedded GraphQL
import { gql } from "graphql-tag";

export const GET_STARSHIPS = gql`
  query GetStarships {
    starships {
      id
      name
      model
      manufacturer
      crew
      passengers
      pilots {
        name
      }
    }
  }
`;
