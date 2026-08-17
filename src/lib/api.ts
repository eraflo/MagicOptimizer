// Typed wrappers around the Tauri commands.
//
// Tauri converts command *argument* names from camelCase on this side to snake_case on the
// Rust side, so `oracleId` here reaches a parameter declared `oracle_id`. That conversion does
// not apply to struct *fields*, which is why src/lib/types.ts follows each type's own serde
// naming rather than one convention throughout.

import { invoke } from "@tauri-apps/api/core";

import type {
  CardDetails,
  CatalogStatus,
  Holding,
  NewHolding,
  Pool,
  SearchRequest,
  SearchResponse,
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
