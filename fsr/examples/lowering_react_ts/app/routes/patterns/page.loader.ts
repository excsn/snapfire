import { fail, type Ctx } from "@snapfire/fsr";

const ORDER = {
  id: "A-17",
  customer: { name: "Ada", address: { city: "Lyon" } as { city: string; country?: string } },
  lines: [
    { sku: "pear", qty: 2, price: 3 },
    { sku: "fig", qty: 1, price: 1.5 },
    { sku: "plum", qty: 4 } as { sku: string; qty: number; price?: number },
  ],
};

export async function load(_: Ctx<"/patterns">) {
  const {
    id,
    customer: { name, address: { city, country = "FR" } },
    lines: [firstLine, ...others],
  } = ORDER;
  for (const { sku, qty, price = 0 } of ORDER.lines) {
    if (qty * price < 0) fail("invalid", `${sku} has a negative total`);
  }
  const head = firstLine.sku, count = others.length;
  return { id, name, city, country, head, count, lines: ORDER.lines };
}
