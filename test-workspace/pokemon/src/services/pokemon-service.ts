import { gql } from '@apollo/client';

export const GET_STARTER_POKEMON = gql`

  query GetStarterPokemon($region: Region!) {
    allPokemon(region: $region, limit: 3) {
      nodes {
        ...PokemonDetailed
        evolution {
          evolvesTo {
            pokemon {
              ...PokemonBasic
            }
            requirement {
              ... on LevelRequirement {
                level
              }
            }
          }
        }
      }
    }
  }
`;

export const SEARCH_POKEMON_BY_TYPE = gql`
  query SearchByType($type: PokemonType!, $limit: Int = 20) {
    allPokemon(type: $type, limit: $limit) {
      nodes {
        id
        name
        number
        types
        stats {
          hp
          attack
          defense
          speed
        }
      }
    }
  }
`;

export const GET_POKEMON_EVOLUTIONS = gql`
  fragment EvolutionChain on Pokemon {
    id
    name
    number
    evolution {
      evolvesFrom {
        id
        name
        number
      }
      evolvesTo {
        pokemon {
          id
          name
        }
        requirement {
          ... on LevelRequirement {
            level
          }
          ... on ItemRequirement {
            item {
              id
              name
            }
          }
        }
      }
    }
  }

  query GetEvolutionChain($id: ID!) {
    pokemon(id: $id) {
      ...EvolutionChain
    }
  }
`;

export interface PokemonService {
  getStarterPokemon(region: string): Promise<any>;
  searchByType(type: string, limit?: number): Promise<any>;
  getEvolutionChain(id: string): Promise<any>;
}
