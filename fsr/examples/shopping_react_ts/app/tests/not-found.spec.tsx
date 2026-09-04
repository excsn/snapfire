import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

test("a path no route matches renders the not-found page with status 404", async () => {
  const { status } = await load("/nowhere/at/all", { ctx: ctx() });
  assert.equal(status, 404);
  assert.ok(screen.getByText("No page at /nowhere/at/all"));
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/not-found.tsx#default"][data-sf-mounted]'), "the page hydrated");
  assert.ok(screen.getByText("Back to the catalog"));
});
