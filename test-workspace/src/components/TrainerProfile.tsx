import React from 'react';
import { gql, useQuery } from '@apollo/client';

const GET_TRAINER_PROFILE = gql`
  query GetTrainerProfile($trainerId: ID!) {
    trainer(id: $trainerId) {
      id
      name
      age
      region
      trainerClass
      stats {
        totalBattles
        wins
        losses
        pokemonCaught
      }
      badges {
        id
        name
        gymLeader
      }
      team {
        nickname
        level
        isShiny
        pokemon {
          id
          name
          types
        }
      }
    }
  }
`;

interface TrainerProfileProps {
  trainerId: string;
}

export const TrainerProfile: React.FC<TrainerProfileProps> = ({ trainerId }) => {
  const { data, loading, error } = useQuery(GET_TRAINER_PROFILE, {
    variables: { trainerId },
  });

  if (loading) return <div>Loading trainer...</div>;
  if (error) return <div>Error: {error.message}</div>;

  const trainer = data?.trainer;

  return (
    <div className="trainer-profile">
      <div className="trainer-header">
        <h1>{trainer.name}</h1>
        <span className="trainer-class">{trainer.trainerClass}</span>
        <span className="region">{trainer.region}</span>
      </div>

      <div className="trainer-stats">
        <h3>Battle Record</h3>
        <p>Battles: {trainer.stats.totalBattles}</p>
        <p>Wins: {trainer.stats.wins}</p>
        <p>Losses: {trainer.stats.losses}</p>
        <p>Pokemon Caught: {trainer.stats.pokemonCaught}</p>
      </div>

      <div className="badges">
        <h3>Badges ({trainer.badges.length})</h3>
        {trainer.badges.map((badge: any) => (
          <div key={badge.id} className="badge">
            {badge.name} - {badge.gymLeader}
          </div>
        ))}
      </div>

      <div className="team">
        <h3>Team</h3>
        {trainer.team.map((teamMember: any, index: number) => (
          <div key={index} className="team-member">
            <h4>
              {teamMember.nickname || teamMember.pokemon.name}
              {teamMember.isShiny && ' ✨'}
            </h4>
            <p>Level {teamMember.level}</p>
            <p>Types: {teamMember.pokemon.types.join(', ')}</p>
          </div>
        ))}
      </div>
    </div>
  );
};
