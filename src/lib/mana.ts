// Splits a Scryfall mana cost into renderable symbols.
//
// Costs arrive as "{2}{W}{U}", with hybrid "{W/U}", monocoloured hybrid "{2/W}", Phyrexian
// "{W/P}", snow "{S}" and variables "{X}". The parser here only needs enough structure to pick
// a colour and a label — mtg-core does the real parsing on the Rust side.

export type ManaSymbol = {
  /** What to draw inside the pip. */
  label: string;
  /** CSS custom property holding the pip colour. */
  colorVar: string;
  /** Hybrid pips get a two-tone background. */
  secondColorVar?: string;
};

const COLOR_VARS: Record<string, string> = {
  W: "--mana-w",
  U: "--mana-u",
  B: "--mana-b",
  R: "--mana-r",
  G: "--mana-g",
};

/** Splits "{2}{W/U}" into its symbols. Unparseable input yields no symbols rather than throwing. */
export function parseManaCost(cost: string): ManaSymbol[] {
  if (!cost) return [];

  const symbols: ManaSymbol[] = [];
  for (const match of cost.matchAll(/\{([^}]*)\}/g)) {
    const body = match[1];
    if (body === "//") continue;

    const parts = body.split("/");
    const colors = parts.filter((p) => p in COLOR_VARS);

    if (colors.length === 2) {
      symbols.push({
        label: colors[0],
        colorVar: COLOR_VARS[colors[0]],
        secondColorVar: COLOR_VARS[colors[1]],
      });
    } else if (colors.length === 1) {
      // Covers plain colours, Phyrexian and monocoloured hybrid alike.
      symbols.push({ label: parts[0] === "2" ? "2" : colors[0], colorVar: COLOR_VARS[colors[0]] });
    } else {
      symbols.push({ label: body, colorVar: "--mana-generic" });
    }
  }
  return symbols;
}

/** Splits a two-part cost so split and adventure cards can be shown as two groups. */
export function splitCostHalves(cost: string): string[] {
  return cost.split(" // ").filter((half) => half.length > 0);
}
