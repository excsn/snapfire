import { enableNavigation } from "@snapfire/fsr-client";
import { ctx, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const invoices = [
  { id: 1n, customer: "Northwind", total: 1250.5, status: "open" },
  { id: 2n, customer: "Contoso", total: 320, status: "overdue" },
];

test("the invoice list renders under the site's prefix with literal links", async () => {
  const c = ctx({ services: { ledger: { listInvoices: () => invoices } } });
  await load("/billing", { ctx: c });
  const anchors = Array.from(document.querySelectorAll("a"));
  const link = anchors.find((a) => a.getAttribute("href") === "/billing/invoice/1");
  expect(link?.textContent, `the site's loader ran through its mocked client and its links carry the prefix as written; anchors: ${anchors.map((a) => a.getAttribute("href")).join(" ")}`).toEqual("Northwind");
  const islands = Array.from(document.querySelectorAll("sf-i")).map((i) => i.getAttribute("data-sf-module"));
  expect(islands.includes("billing:routes/layout.tsx#default"), "the site's layout is an island under its prefixed id").toBeTruthy();
});

test("a second enableNavigation, which a mounted site's entry makes, keeps the page a navigation installed", async () => {
  const c = ctx({ identity: { subject: "alice", claims: {} }, services: { ledger: { listInvoices: () => invoices } } });
  await load("/billing", { ctx: c });
  const nav = document.querySelector(".billing-nav");
  await fireEvent.click(document.querySelector('a[href="/billing/overdue"]')!);
  await settle();
  expect(document.querySelector("h1")?.textContent).toEqual("Overdue");
  enableNavigation();
  await fireEvent.click(document.querySelector('a[href="/billing"]')!);
  await settle();
  expect(document.querySelector("h1")?.textContent, "the click after the repeat call patched the page in place").toEqual("Invoices");
  expect(document.title).toEqual("Invoices · Billing");
  expect(document.querySelector(".billing-nav") === nav, "the layout kept its DOM across both clicks").toBeTruthy();
});
