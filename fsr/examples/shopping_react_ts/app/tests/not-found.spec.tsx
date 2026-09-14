import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

test("a path no route matches renders the not-found page with status 404", async () => {
  const { status } = await load("/nowhere/at/all", { ctx: ctx({ services: { shopping: { listProducts: () => [] } } }) });
  expect(status).toEqual(404);
  expect(screen.getByText("No page at /nowhere/at/all")).toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/not-found.tsx#default"]'), "the page has no state, so it is markup rather than an island").toBeNull();
  expect(screen.getByText("Back to the catalog")).toBeTruthy();
});
