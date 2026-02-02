// Example with backtick on separate line (some formatters do this)
import { gql } from "graphql-tag";

export const GET_PLANET_CLIMATES = gql
  `
  query PlanetClimates {
    planets {
      name
      climate
    }
  }
`;
