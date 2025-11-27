import React from 'react';
import { gql, useQuery } from '@apollo/client';

const GET_POKEMON_CARD = gql`
  query GetPokemonCard($id: ID!) {
    pokemon(id: $id) {
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
      abilities {
        id
        name
        isHidden
      }
      isLegendary
      isMythical
    }
  }
`;

interface PokemonCardProps {
  pokemonId: string;
}

export const PokemonCard: React.FC<PokemonCardProps> = ({ pokemonId }) => {
  const { data, loading, error } = useQuery(GET_POKEMON_CARD, {
    variables: { id: pokemonId },
  });

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  const pokemon = data?.pokemon;

  return (
    <div className="pokemon-card">
      <h2>
        #{pokemon.number} {pokemon.name}
      </h2>
      <div className="types">
        {pokemon.types.map((type: string) => (
          <span key={type} className={`type ${type.toLowerCase()}`}>
            {type}
          </span>
        ))}
      </div>
      <div className="stats">
        <div>HP: {pokemon.stats.hp}</div>
        <div>Attack: {pokemon.stats.attack}</div>
        <div>Defense: {pokemon.stats.defense}</div>
        <div>Speed: {pokemon.stats.speed}</div>
      </div>
      {pokemon.isLegendary && <span className="badge legendary">Legendary</span>}
      {pokemon.isMythical && <span className="badge mythical">Mythical</span>}
    </div>
  );
};
