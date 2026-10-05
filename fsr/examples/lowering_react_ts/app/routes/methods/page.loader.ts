import type { Ctx } from "@snapfire/fsr";

const SHELF = [
  { name: "pear", price: 3, tags: ["fruit", "green"] },
  { name: "fig", price: 1.5, tags: ["fruit"] },
  { name: "kiwi", price: 3, tags: ["fruit", "green", "fuzzy"] },
  { name: "plum", price: 0.5, tags: [] },
];

export async function load({ query }: Ctx<"/methods">) {
  const take = Number(query.take ?? "3");
  const cheapest = [...SHELF].sort((a, b) => a.price - b.price).slice(0, take);
  const tags = SHELF.flatMap((item) => item.tags).filter((tag, i, all) => all.indexOf(tag) === i);
  const priciest = Math.max(...SHELF.map((item) => item.price));
  return {
    cheapest: cheapest.map((item) => item.name),
    tags,
    priciest,
    last: SHELF.at(-1)?.name ?? "",
    codes: SHELF.map((item) => item.name.slice(0, 2).toUpperCase().padEnd(4, "-")),
    json: JSON.stringify({ take, first: cheapest[0]?.name ?? null }),
    squared: take ** 2,
  };
}
