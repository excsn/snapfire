import { assert, ctx, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const agents = [{ id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 }];

test("the tips island is placed with idle timing and mounts in its own root", async () => {
  const c = ctx({ session: { watching: {} }, services: { fleet: { listAgents: () => agents, listAlerts: () => [] } } });
  await load("/", { ctx: c });
  const region = document.querySelector('sf-s[data-sf-island][data-sf-when="idle"]');
  assert.ok(region, "the server rendered the island in a region timed on idle");
  assert.ok(region!.querySelector("details.tips"), "with its markup");
  await settle();
  assert.ok(region!.querySelector('sf-i[data-sf-module="src/ui/Tips.tsx#TipList"][data-sf-mounted]'), "mounted in its own root once the thread was idle");
  assert.ok(screen.getByText("Nothing is on fire."), "the alerts slot resolved to its empty state");
  await settle();
});
