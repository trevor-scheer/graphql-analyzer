export enum PokemonType {
  NORMAL = 'NORMAL',
  FIRE = 'FIRE',
  WATER = 'WATER',
  ELECTRIC = 'ELECTRIC',
  GRASS = 'GRASS',
  ICE = 'ICE',
  FIGHTING = 'FIGHTING',
  POISON = 'POISON',
  GROUND = 'GROUND',
  FLYING = 'FLYING',
  PSYCHIC = 'PSYCHIC',
  BUG = 'BUG',
  ROCK = 'ROCK',
  GHOST = 'GHOST',
  DRAGON = 'DRAGON',
  DARK = 'DARK',
  STEEL = 'STEEL',
  FAIRY = 'FAIRY',
}

export enum Region {
  KANTO = 'KANTO',
  JOHTO = 'JOHTO',
  HOENN = 'HOENN',
  SINNOH = 'SINNOH',
  UNOVA = 'UNOVA',
  KALOS = 'KALOS',
  ALOLA = 'ALOLA',
  GALAR = 'GALAR',
  PALDEA = 'PALDEA',
}

export enum TrainerClass {
  ACE_TRAINER = 'ACE_TRAINER',
  YOUNGSTER = 'YOUNGSTER',
  LASS = 'LASS',
  BUG_CATCHER = 'BUG_CATCHER',
  SWIMMER = 'SWIMMER',
  HIKER = 'HIKER',
  BREEDER = 'BREEDER',
  CHAMPION = 'CHAMPION',
  GYM_LEADER = 'GYM_LEADER',
  ELITE_FOUR = 'ELITE_FOUR',
}

export interface Pokemon {
  id: string;
  name: string;
  number: number;
  types: PokemonType[];
  stats: Stats;
  height: number;
  weight: number;
  isLegendary: boolean;
  isMythical: boolean;
}

export interface Stats {
  hp: number;
  attack: number;
  defense: number;
  specialAttack: number;
  specialDefense: number;
  speed: number;
  total: number;
}

export interface Trainer {
  id: string;
  name: string;
  region: Region;
  trainerClass: TrainerClass;
  team: TeamPokemon[];
}

export interface TeamPokemon {
  pokemon: Pokemon;
  nickname?: string;
  level: number;
  experience: number;
  friendship: number;
  isShiny: boolean;
}
