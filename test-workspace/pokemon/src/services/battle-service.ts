import { gql } from "@apollo/client";

export const START_NEW_BATTLE = gql`
  mutation InitiateBattle($trainer1: ID!, $trainer2: ID!) {
    startBattle(trainer1Id: $trainer1, trainer2Id: $trainer2) {
      id
      status
      startedAt
      trainer1 {
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
        }
      }
      trainer2 {
        id
        name
        team {
          nickname
          level
          pokemon {
            name
            types
          }
        }
      }
    }
  }
`;

export const PERFORM_ATTACK = gql`
  mutation Attack($battleId: ID!, $trainerId: ID!, $moveId: ID!) {
    performBattleAction(
      battleId: $battleId
      trainerId: $trainerId
      action: { type: ATTACK, moveId: $moveId }
    ) {
      id
      status
      turns {
        turnNumber
        trainer {
          name
        }
        action {
          ... on AttackAction {
            move {
              name
              type
            }
            damage
            wasEffective
          }
        }
      }
    }
  }
`;

export const GET_ACTIVE_BATTLES = gql`
  query ActiveBattles {
    activeBattles {
      id
      status
      startedAt
      trainer1 {
        id
        name
      }
      trainer2 {
        id
        name
      }
    }
  }
`;

export const GET_BATTLE_HISTORY = gql`
  query BattleHistory($trainerId: ID!) {
    trainerBattles(trainerId: $trainerId) {
      id
      status
      startedAt
      endedAt
      winner {
        id
        name
      }
      trainer1 {
        id
        name
      }
      trainer2 {
        id
        name
      }
    }
  }
`;

export const SWITCH_POKEMON_IN_BATTLE = gql`
  mutation SwitchPokemonInBattle($battleId: ID!, $trainerId: ID!, $newPokemonId: ID!) {
    performBattleAction(
      battleId: $battleId
      trainerId: $trainerId
      action: { type: SWITCH, switchToPokemonId: $newPokemonId }
    ) {
      id
      turns {
        turnNumber
        action {
          ... on SwitchAction {
            fromPokemon {
              pokemon {
                name
              }
            }
            toPokemon {
              pokemon {
                ...PokemonName
              }
            }
          }
        }
      }
    }
  }
`;

const frag = gql`
  fragment PokemonName on Pokemon {
    name
  }
`;
