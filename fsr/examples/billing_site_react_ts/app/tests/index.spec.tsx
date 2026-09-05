import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const invoices = [
  { id: 1n, customer: "Northwind", total: 1250.5, status: "open" },
  { id: 2n, customer: "Contoso", total: 320.5, status: "overdue" },
];

test("the invoice list renders under the site's prefix with literal links", async () => {
  const c = ctx({ services: { ledger: { listInvoices: () => invoices } } });
  await load("/billing", { ctx: c });
  const anchors = Array.from(document.querySelectorAll("a"));
  const link = anchors.find((a) => a.getAttribute("href") === "/billing/invoice/1");
  assert.equal(link?.textContent, "Northwind", `the site's loader ran through its mocked client and its links carry the prefix as written; anchors: ${anchors.map((a) => a.getAttribute("href")).join(" ")}`);
  const islands = Array.from(document.querySelectorAll("sf-i")).map((i) => i.getAttribute("data-sf-module"));
  assert.ok(islands.includes("billing:routes/layout.tsx#default"), "the site's layout is an island under its prefixed id");
});
