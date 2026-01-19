import { gql } from "@apollo/client";

export const CREATE_NEW_TRAINER = gql`
  mutation CreateNewTrainer($name: String!, $region: Region!, $class: TrainerClass!) {
    createTrainer(input: { name: $name, region: $region, trainerClass: $class }) {
      id
      name
      region
      trainerClass
      stats {
        totalBattles
        wins
        losses
        pokemonCaught
      }
    }
  }
`;

export const ADD_POKEMON_TO_TEAM = gql`
  mutation AddToTeam($trainerId: ID!, $pokemonId: ID!, $nickname: String, $level: Int!) {
    addPokemonToTeam(
      trainerId: $trainerId
      input: { pokemonId: $pokemonId, nickname: $nickname, level: $level, isShiny: false }
    ) {
      id
      name
      team {
        nickname
        level
        pokemon {
          id
          name
          types
        }
        moves {
          id
          name
          type
        }
      }
    }
  }
`;

export const GET_TRAINER_WITH_TEAM = gql`
  query GetTrainerTeam($trainerId: ID!) {
    trainer(id: $trainerId) {
      id
      name
      region
      trainerClass
      team {
        nickname
        level
        experience
        friendship
        isShiny
        pokemon {
          id
          name
          number
          types
          stats {
            hp
            attack
            defense
            specialAttack
            specialDefense
            speed
          }
        }
        heldItem {
          id
          name
          category
        }
        moves {
          id
          name
          type
          category
          power
          pp
        }
      }
      badges {
        id
        name
        gymLeader
      }
    }
  }
`;

export const CATCH_WILD_POKEMON = gql`
  mutation CatchWildPokemon($trainerId: ID!, $pokemonId: ID!, $pokeballId: ID!) {
    catchPokemon(trainerId: $trainerId, pokemonId: $pokemonId, pokeball: $pokeballId) {
      success
      message
      pokemon {
        nickname
        level
        pokemon {
          id
          name
          types
        }
      }
    }
  }
`;
