// Typed wrappers around the Tauri commands.
//
// Tauri converts command *argument* names from camelCase on this side to snake_case on the
// Rust side, so `oracleId` here reaches a parameter declared `oracle_id`. That conversion does
// not apply to struct *fields*, which is why src/lib/types.ts follows each type's own serde
// naming rather than one convention throughout.

import { invoke } from "@tauri-apps/api/core";

import type {
  BracketAssessment,
  CardDetails,
  CatalogStatus,
  ComboMatch,
  ComboStatus,
  DeckView,
  ExportStyle,
  ImportOutcome,
  StoredDeck,
  Zone,
  Holding,
  NewHolding,
  Pool,
  OptimizerOptions,
  Score,
  SearchRequest,
  SearchResponse,
  DeckHistory,
  Game,
  GameInput,
  ScanResult,
  ScanStatus,
  SearchResult,
  Stats,
} from "./types";

export function catalogStatus(): Promise<CatalogStatus> {
  return invoke("catalog_status");
}

export function reloadCatalog(): Promise<CatalogStatus> {
  return invoke("reload_catalog");
}

export function searchCards(request: SearchRequest): Promise<SearchResponse> {
  return invoke("search_cards", { request });
}

export function cardDetails(oracleId: string): Promise<CardDetails> {
  return invoke("card_details", { oracleId });
}

export function cardByName(name: string): Promise<CardDetails> {
  return invoke("card_by_name", { name });
}

export function collectionList(pool: Pool | "all"): Promise<Holding[]> {
  return invoke("collection_list", { pool });
}

export function collectionAdd(holding: NewHolding): Promise<number> {
  return invoke("collection_add", { holding });
}

export function collectionSetQuantity(id: number, quantity: number): Promise<void> {
  return invoke("collection_set_quantity", { id, quantity });
}

export function collectionUpdate(holding: Holding): Promise<void> {
  return invoke("collection_update", { holding });
}

export function collectionRemove(id: number): Promise<boolean> {
  return invoke("collection_remove", { id });
}

export function collectionStats(pool: Pool | "all"): Promise<Stats> {
  return invoke("collection_stats", { pool });
}

export function collectionOwnedQuantities(
  pool: Pool | "all",
): Promise<Record<string, number>> {
  return invoke("collection_owned_quantities", { pool });
}

export function collectionContainers(): Promise<string[]> {
  return invoke("collection_containers");
}

/** Format keys and display names, served from Rust so the two cannot drift apart. */
export function formats(): Promise<[string, string][]> {
  return invoke("formats");
}

// --- Decks -----------------------------------------------------------------

export function deckList(): Promise<StoredDeck[]> {
  return invoke("deck_list");
}

export function deckGet(id: number): Promise<DeckView> {
  return invoke("deck_get", { id });
}

export function deckCreate(name: string, format: string): Promise<number> {
  return invoke("deck_create", { name, format });
}

export function deckDelete(id: number): Promise<boolean> {
  return invoke("deck_delete", { id });
}

export function deckRename(id: number, name: string, format: string): Promise<DeckView> {
  return invoke("deck_rename", { id, name, format });
}

export function deckAddCard(
  id: number,
  oracleId: string,
  quantity: number,
  zone: Zone,
): Promise<DeckView> {
  return invoke("deck_add_card", { id, oracleId, quantity, zone });
}

export function deckRemoveCard(
  id: number,
  oracleId: string,
  quantity: number,
  zone: Zone,
): Promise<DeckView> {
  return invoke("deck_remove_card", { id, oracleId, quantity, zone });
}

export function deckMoveCard(
  id: number,
  oracleId: string,
  quantity: number,
  from: Zone,
  to: Zone,
): Promise<DeckView> {
  return invoke("deck_move_card", { id, oracleId, quantity, from, to });
}

export function deckImport(text: string, name: string, format: string): Promise<ImportOutcome> {
  return invoke("deck_import", { text, name, format });
}

export function deckExport(id: number, style: ExportStyle): Promise<string> {
  return invoke("deck_export", { id, style });
}

export function deckZones(): Promise<[Zone, string][]> {
  return invoke("deck_zones");
}

// --- Optimizer -------------------------------------------------------------

export function deckScore(id: number, archetype: string): Promise<Score> {
  return invoke("deck_score", { id, archetype });
}

export function deckOptimize(
  id: number,
  archetype: string,
  pool: string,
  iterations?: number,
  onlyPlayedCards?: boolean,
): Promise<SearchResult> {
  return invoke("deck_optimize", { id, archetype, pool, iterations, onlyPlayedCards });
}

export function deckApplySuggestion(
  id: number,
  removeOracleId: string,
  addOracleId: string,
): Promise<DeckView> {
  return invoke("deck_apply_suggestion", { id, removeOracleId, addOracleId });
}

/** Archetypes and card pools the optimizer offers. */
export function optimizerOptions(): Promise<OptimizerOptions> {
  return invoke("optimizer_options");
}

// --- Combos and brackets ---------------------------------------------------

export function comboStatus(): Promise<ComboStatus> {
  return invoke("combo_status");
}

export function deckCombos(id: number): Promise<ComboMatch[]> {
  return invoke("deck_combos", { id });
}

export function deckBracket(id: number): Promise<BracketAssessment> {
  return invoke("deck_bracket", { id });
}

// --- Camera scanning -------------------------------------------------------

export function scanStatus(): Promise<ScanStatus> {
  return invoke("scan_status");
}

export function scanReload(): Promise<ScanStatus> {
  return invoke("scan_reload");
}

export function scanReset(): Promise<void> {
  return invoke("scan_reset");
}

/**
 * Feeds one greyscale frame to the scanner.
 *
 * Sent as a raw body rather than a normal argument. A 640x480 frame is 300 KB, and passing it
 * as a `number[]` would have Tauri serialize three hundred thousand JSON numbers ten times a
 * second — the encoding would cost far more than the recognition does. The dimensions ride
 * along as headers because a raw body cannot carry anything else.
 */
export function scanFrame(gray: Uint8Array, width: number, height: number): Promise<ScanResult> {
  return invoke("scan_frame", gray, {
    headers: { width: String(width), height: String(height) },
  });
}

// --- Game log --------------------------------------------------------------

export function journalAdd(game: GameInput): Promise<number> {
  return invoke("journal_add", { game });
}

export function journalRemove(id: number): Promise<boolean> {
  return invoke("journal_remove", { id });
}

export function journalList(): Promise<Game[]> {
  return invoke("journal_list");
}

/**
 * One deck's games and everything derived from them, in a single call.
 *
 * One round trip rather than four, so the view cannot show a win rate and a matchup table that
 * disagree with each other.
 */
export function journalDeckHistory(deckId: number, since?: string): Promise<DeckHistory> {
  return invoke("journal_deck_history", { deckId, since: since ?? null });
}
