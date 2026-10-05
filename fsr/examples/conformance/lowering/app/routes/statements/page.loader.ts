import type { Ctx } from "@snapfire/fsr";

const STOCK = [
  { sku: "pear", left: 0, kind: "fruit" },
  { sku: "fig", left: 2, kind: "fruit" },
  { sku: "leek", left: 9, kind: "veg" },
  { sku: "salt", left: 40, kind: "pantry" },
];

export async function load({ query }: Ctx<"/statements">) {
  let total = 0;
  let low = 0;
  const kinds: string[] = [];
  for (const item of STOCK) {
    total += item.left;
    if (item.left < 3) low++;
    switch (item.kind) {
      case "fruit":
      case "veg":
        kinds.push(`fresh ${item.sku}`);
        break;
      default:
        kinds.push(`dry ${item.sku}`);
    }
  }
  let note = "steady";
  if (low > 1) note = "reorder";
  return { total, low, kinds, note, items: STOCK, pick: query.pick ?? "" };
}
