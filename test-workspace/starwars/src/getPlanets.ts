// Example TypeScript file with embedded GraphQL
import { gql } from "graphql-tag";

export const GET_PLANETS = gql`
  query AllPlanets {
    planets {
      id
      name
      climate
      population
      residents {
        name
      }
    }
  }
`;
