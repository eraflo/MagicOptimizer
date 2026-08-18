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
  /**
   * What the card does, as readable labels ("Removal", "Ramp", ...).
   * Empty means nothing is known — the tagger is crowdsourced — not that the card does nothing.
   */
  tags: string[];
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

// --- Decks -----------------------------------------------------------------
// Snake_case throughout: these come from mtg-deck with serde's default naming.
// `format` is a Scryfall key ("commander"), not a variant name — see mtg_core::Format.

export type Zone = "main" | "sideboard" | "command";

export type DeckEntry = {
  oracle_id: string;
  name: string;
  quantity: number;
  zone: Zone;
  set_code: string;
  collector_number: string;
};

export type Deck = {
  name: string;
  format: string;
  entries: DeckEntry[];
  notes: string;
};

/** StoredDeck flattens the deck into itself, so the id sits alongside the deck's own fields. */
export type StoredDeck = Deck & { id: number };

/** Tagged by `kind`; the remaining fields depend on which one it is. */
export type Violation = { kind: string } & Record<string, unknown>;

export type LegalityReport = {
  format: string;
  approximate_rules: boolean;
  violations: Violation[];
  main_count: number;
  sideboard_count: number;
  command_count: number;
  commander_identity: string;
};

export type CurveBucket = {
  mana_value: number;
  count: number;
  is_overflow: boolean;
};

export type ColorPips = { color: string; pips: number; cards: number };

export type DeckStats = {
  total_cards: number;
  lands: number;
  creatures: number;
  curve: CurveBucket[];
  average_mana_value: number;
  color_pips: ColorPips[];
  color_identity: string;
  unresolved_cards: number;
};

export type DeckView = {
  id: number;
  deck: Deck;
  legality: LegalityReport;
  stats: DeckStats;
};

export type ImportOutcome = {
  view: DeckView;
  problems: unknown[];
  /** Already-rendered messages, so the UI need not know each problem's shape. */
  messages: string[];
};

export type ExportStyle = "plain" | "arena" | "mtgo";

/** Turns a violation into a sentence. Mirrors the Display impl on the Rust side. */
export function describeViolation(violation: Violation): string {
  const v = violation as Record<string, string | number>;
  switch (violation.kind) {
    case "unknown_card":
      return `${v.name} is not in the loaded card data`;
    case "deck_too_small":
      return `the deck has ${v.found} cards, ${v.required} are required`;
    case "deck_too_large":
      return `the deck has ${v.found} cards, only ${v.allowed} are allowed`;
    case "sideboard_too_large":
      return `the sideboard has ${v.found} cards, only ${v.allowed} are allowed`;
    case "sideboard_not_allowed":
      return `this format has no sideboard, but ${v.found} cards are in one`;
    case "too_many_copies":
      return `${v.found} copies of ${v.name}, only ${v.allowed} allowed`;
    case "not_in_format":
      return `${v.name} is not legal in this format`;
    case "banned":
      return `${v.name} is banned`;
    case "restricted":
      return `${v.name} is restricted to one copy, the deck has ${v.found}`;
    case "outside_color_identity":
      return `${v.name} has colour identity ${v.card_identity}, outside the commander's ${v.commander_identity}`;
    case "command_zone_size":
      return v.minimum === v.maximum
        ? `the command zone has ${v.found} cards, ${v.minimum} is required`
        : `the command zone has ${v.found} cards, between ${v.minimum} and ${v.maximum} are required`;
    case "not_a_valid_commander":
      return `${v.name} cannot be a commander`;
    case "command_zone_not_allowed":
      return `this format has no command zone, but ${v.found} cards are in one`;
    default:
      return violation.kind;
  }
}

// --- Optimizer -------------------------------------------------------------

export type Criterion = {
  name: string;
  /** Always 0..1, so criteria are comparable. */
  score: number;
  weight: number;
  detail: string;
  /** False for the criteria that encode a convention rather than a calculation. */
  derived: boolean;
};

export type SimulationResult = {
  games: number;
  keepable_opening_hands: number;
  average_mulligans: number;
  average_opening_lands: number;
  land_drops_made: number[];
  on_curve_by_turn: number[];
  mana_screw: number;
  mana_flood: number;
};

export type Score = {
  /** Weighted average of the criteria, 0–100. */
  total: number;
  criteria: Criterion[];
  simulation: SimulationResult;
  /** False when the deck holds cards the catalog could not resolve. */
  reliable: boolean;
  unresolved_cards: number;
};

export type Suggestion = {
  remove_oracle_id: string;
  remove_name: string;
  add_oracle_id: string;
  add_name: string;
  score_before: number;
  score_after: number;
  reasons: string[];
};

export type SearchResult = {
  before: Score;
  after: Score;
  suggestions: Suggestion[];
  candidates_considered: number;
};

export type Choice = { key: string; label: string };

export type OptimizerOptions = {
  archetypes: Choice[];
  pools: Choice[];
};

// --- Combos and brackets ---------------------------------------------------

export type ComboMatch = {
  /** Commander Spellbook's identifier, so it can be looked up on their site. */
  id: string;
  card_names: string[];
  produces: string[];
  card_count: number;
  is_infinite: boolean;
  wins_the_game: boolean;
};

export type Marker = { name: string; note: string };

export type BracketAssessment = {
  /** Between 2 and 4: brackets 1 and 5 depend on intent, not on contents. */
  bracket: number;
  reasons: string[];
  game_changers: Marker[];
  two_card_combos: ComboMatch[];
  longer_combos: ComboMatch[];
  mass_land_denial: Marker[];
  extra_turns: Marker[];
  tutors: Marker[];
  /** What the estimate could not check. Shown, not hidden. */
  caveats: string[];
};

export type ComboStatus = {
  loaded: boolean;
  combos: number;
  fetchedAt: string;
  path: string;
  error: string | null;
};

export const BRACKET_LABELS: Record<number, string> = {
  1: "Exhibition",
  2: "Core",
  3: "Upgraded",
  4: "Optimized",
  5: "cEDH",
};

// --- Camera scanning -------------------------------------------------------

export type ScanStatus = {
  loaded: boolean;
  artworks: number;
  path: string;
  error: string | null;
};

export type ScannedCard = {
  oracleId: string;
  printingId: string;
  name: string;
  /** Bits of difference from the reference artwork. Lower is closer. */
  distance: number;
  /** How much worse the nearest different card was. Larger is more certain. */
  margin: number;
};

export type ScanResult = {
  /**
   * `searching` — nothing recognised.
   * `tracking` — a card matches but not enough frames agree yet.
   * `confirmed` — act on this, emitted exactly once per card presented.
   * `holding` — the confirmed card is still in view; do nothing.
   */
  state: "searching" | "tracking" | "confirmed" | "holding";
  card: ScannedCard | null;
  votes: number;
  needed: number;
  /**
   * The card's outline in frame coordinates, top-left first and clockwise.
   * Present whenever a card was *seen*, even if it could not be named.
   */
  quad: [number, number][] | null;
};
