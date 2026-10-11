import { ctx, expect, load, test } from "@snapfire/fsr-client/testing";

const reports = [
  { id: 1n, name: "Revenue by region", period: "Q3", total: 48210.5 },
  { id: 2n, name: "Sign-ups by channel", period: "Q3", total: 1844 },
];

test("the report list renders under the site's prefix from its own client", async () => {
  const c = ctx({ services: { metrics: { listReports: () => reports } } });
  await load("/reports", { ctx: c });
  const link = Array.from(document.querySelectorAll("a")).find((a) => a.getAttribute("href") === "/reports/1");
  expect(link?.textContent, "the loader ran through the mocked metrics client and the link carries the prefix as written").toEqual("Revenue by region");
  const islands = Array.from(document.querySelectorAll("sf-i")).map((i) => i.getAttribute("data-sf-module"));
  expect(islands.includes("reports:routes/layout.tsx#default"), "the site's layout is an island under its prefixed id").toBeTruthy();
});
