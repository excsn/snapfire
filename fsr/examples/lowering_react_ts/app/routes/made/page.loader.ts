import type { Ctx } from "@snapfire/fsr";

const DELIVERIES = [
  { id: "d1", at: "2026-10-01T09:30:00Z", kind: "fruit", n: 3, price: 4.5 },
  { id: "d2", at: "2026-10-03T17:05:00Z", kind: "veg", n: 2, price: 3 },
  { id: "d3", at: "2026-09-29T07:00:00Z", kind: "fruit", n: 5, price: 1.25 },
];

export async function load({ query }: Ctx<"/made">) {
  const now = Date.now();
  const kinds = new Set<string>();
  const totals = new Map<string, number>();
  for (const delivery of DELIVERIES) {
    kinds.add(delivery.kind);
    totals.set(delivery.kind, (totals.get(delivery.kind) ?? 0) + delivery.n);
  }
  const latest = [...DELIVERIES].sort((a, b) => Date.parse(b.at) - Date.parse(a.at))[0];
  const spent = DELIVERIES.reduce((sum, d) => sum + d.n * d.price, 0);
  return {
    kinds: [...kinds],
    totals: Object.fromEntries(totals),
    count: totals.size,
    latest: Date.parse(latest.at),
    past: now > Date.parse(latest.at),
    spent,
    link: `/made?${new URLSearchParams({ kind: query.kind ?? "fruit", q: "tea & cake" })}`,
  };
}
