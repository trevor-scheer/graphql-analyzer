import React from "react";
import { gql } from "@apollo/client";
import { useQuery, useSubscription } from "@apollo/client/react";

const GET_BATTLE = gql`
  query GetBattleDetails($battleId: ID!) {
    battle(id: $battleId) {
      id
      status
      startedAt
      endedAt
      trainer1 {
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
      winner {
        id
        name
      }
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
          ... on SwitchAction {
            fromPokemon {
              pokemon {
                name
              }
            }
            toPokemon {
              pokemon {
                name
              }
            }
          }
        }
      }
    }
  }
`;

const BATTLE_UPDATED = gql`
  subscription OnBattleUpdated($battleId: ID!) {
    battleUpdated(battleId: $battleId) {
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
            }
            damage
          }
        }
      }
    }
  }
`;

interface BattleViewerProps {
  battleId: string;
}

export const BattleViewer: React.FC<BattleViewerProps> = ({ battleId }) => {
  const { data, loading, error } = useQuery<any>(GET_BATTLE, {
    variables: { battleId },
  });

  const { data: subscriptionData } = useSubscription<any>(BATTLE_UPDATED, {
    variables: { battleId },
  });

  if (loading) return <div>Loading battle...</div>;
  if (error) return <div>Error: {error.message}</div>;

  const battle = subscriptionData?.battleUpdated || data?.battle;

  return (
    <div className="battle-viewer">
      <div className="battle-header">
        <h2>Battle #{battle.id}</h2>
        <span className={`status ${battle.status.toLowerCase()}`}>{battle.status}</span>
      </div>

      <div className="trainers">
        <div className="trainer">
          <h3>{battle.trainer1.name}</h3>
          <div className="team">
            {battle.trainer1.team.map((member: any, idx: number) => (
              <div key={idx}>
                {member.nickname || member.pokemon.name} (Lv. {member.level})
              </div>
            ))}
          </div>
        </div>

        <div className="vs">VS</div>

        <div className="trainer">
          <h3>{battle.trainer2.name}</h3>
          <div className="team">
            {battle.trainer2.team.map((member: any, idx: number) => (
              <div key={idx}>
                {member.nickname || member.pokemon.name} (Lv. {member.level})
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="battle-log">
        <h3>Battle Log</h3>
        {battle.turns.map((turn: any) => (
          <div key={turn.turnNumber} className="turn">
            <strong>Turn {turn.turnNumber}:</strong> {turn.trainer.name}
            {turn.action.__typename === "AttackAction" && (
              <span>
                {" "}
                used {turn.action.move.name} for {turn.action.damage} damage! (
                {turn.action.wasEffective})
              </span>
            )}
            {turn.action.__typename === "SwitchAction" && (
              <span>
                {" "}
                switched from {turn.action.fromPokemon.pokemon.name} to{" "}
                {turn.action.toPokemon.pokemon.name}
              </span>
            )}
          </div>
        ))}
      </div>

      {battle.winner && (
        <div className="winner">
          <h3>Winner: {battle.winner.name}!</h3>
        </div>
      )}
    </div>
  );
};
