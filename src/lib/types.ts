// Mirrors the Rust types in src-tauri/src/dto.rs and the mtg-collection crate.
//
// Field naming is not uniform and that is not an accident: SearchRequest is declared
// `rename_all = "camelCase"` on the Rust side, while collection types use serde's default
// snake_case because they are the crate's own persisted shape. Each block below matches
// whichever convention its Rust counterpart actually uses.

export type CardSummary = {
  oracle_id: string;
  name: string;
  mana_cost: string;
  mana_value: number;
  type_line: string;
  colors: string;
  color_identity: string;
  set_code: string;
  collector_number: string;
  game_changer: boolean;
  edhrec_rank: number | null;
  faces: number;
  image_small: string | null;
};

export type FaceView = {
  name: string;
  mana_cost: string;
  type_line: string;
  oracle_text: string;
  power: string | null;
  toughness: string | null;
  loyalty: string | null;
};

export type CardDetails = CardSummary & {
  oracle_text: string;
  power: string | null;
  toughness: string | null;
  loyalty: string | null;
  keywords: string[];
  rarity: string;
  reserved: boolean;
  layout: string;
  released_at: string;
  legal_formats: string[];
  banned_formats: string[];
  restricted_formats: string[];
  face_views: FaceView[];
  image_normal: string | null;
};

// camelCase: matches `#[serde(rename_all = "camelCase")]` on SearchRequest.
export type SearchRequest = {
  text?: string | null;
  cardTypes?: string[];
  identity?: string | null;
  format?: string | null;
  minManaValue?: number | null;
  maxManaValue?: number | null;
  gameChangersOnly?: boolean;
  commandersOnly?: boolean;
  ownedOnly?: boolean;
  limit?: number | null;
};

export type SearchResponse = {
  total: number;
  cards: CardSummary[];
};

export type CatalogStatus = {
  loaded: boolean;
  cards: number;
  sourceUpdatedAt: string;
  path: string;
  error: string | null;
};

export type Pool = "physical" | "digital";
export type Finish = "nonfoil" | "foil" | "etched";
export type Condition =
  | "near_mint"
  | "lightly_played"
  | "moderately_played"
  | "heavily_played"
  | "damaged";

export type StorageLocation = {
  container: string;
  section?: string | null;
  slot?: number | null;
};

export type Holding = {
  id: number;
  pool: Pool;
  oracle_id: string;
  name: string;
  set_code: string;
  collector_number: string;
  language: string;
  finish: Finish;
  condition: Condition;
  quantity: number;
  location?: StorageLocation | null;
  notes: string;
};

export type NewHolding = Omit<Holding, "id">;

export type Stats = {
  holdings: number;
  distinct_cards: number;
  total_copies: number;
};

export const CONDITIONS: { value: Condition; label: string }[] = [
  { value: "near_mint", label: "Near Mint" },
  { value: "lightly_played", label: "Lightly Played" },
  { value: "moderately_played", label: "Moderately Played" },
  { value: "heavily_played", label: "Heavily Played" },
  { value: "damaged", label: "Damaged" },
];

export const FINISHES: { value: Finish; label: string }[] = [
  { value: "nonfoil", label: "Non-foil" },
  { value: "foil", label: "Foil" },
  { value: "etched", label: "Etched" },
];

// Scryfall language codes, limited to the ones printed on paper.
export const LANGUAGES: { value: string; label: string }[] = [
  { value: "en", label: "English" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "es", label: "Spanish" },
  { value: "pt", label: "Portuguese" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
  { value: "ru", label: "Russian" },
  { value: "zhs", label: "Chinese (Simplified)" },
  { value: "zht", label: "Chinese (Traditional)" },
];
