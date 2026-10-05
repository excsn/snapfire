import type { Ctx } from "@snapfire/fsr";

const SLUG = /[^a-z0-9]+/g;
const TITLES = ["Pears & Figs: a Field Guide", "Leeks, Onions and Other Alliums", "Salt (Coarse) vs. Fine"];

export async function load({ query }: Ctx<"/regex">) {
  const term = query.q ?? "and";
  return {
    entries: TITLES.map((title) => ({ title, slug: title.toLowerCase().replace(SLUG, "-").replace(/^-|-$/g, "") })),
    term,
    years: "since 1998, revised 2004 and 2019".match(/\d{4}/g) ?? [],
  };
}
