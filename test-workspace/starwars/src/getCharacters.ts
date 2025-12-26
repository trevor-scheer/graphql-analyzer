// Example TypeScript file with embedded GraphQL
import { gql } from "graphql-tag";

export const GET_CHARACTERS = gql`
  query AllCharacters {
    characters {
      id
      name
      species
      homeworld {
        name
      }
      starships {
        name
      }
      affiliation
    }
  }
`;
