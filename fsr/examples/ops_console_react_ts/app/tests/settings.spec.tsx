import { get } from "@snapfire/fsr-client";
import { assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

import { density, watching } from "@src/store";

const agents = [{ id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 }];

function services() {
  return { fleet: { listAgents: () => agents, listAlerts: () => [] } };
}

test("the gear opens settings in the root layout's drawer, and a document load is the whole page", async () => {
  const c = ctx({ session: { watching: { "1": true }, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const drawer = document.querySelector('sf-s[data-sf-name="drawer"]')!;
  assert.ok(drawer.querySelector(".drawer-hint"));

  await fireEvent.click(screen.getByLabelText("Settings"));
  assert.equal(location.pathname, "/settings");
  assert.ok(drawer.querySelector('sf-i[data-sf-module="routes/settings/page.drawer.tsx#default"][data-sf-mounted]'), "the drawer variant hydrated in the root layout's slot");
  assert.ok(drawer.querySelector(".watch-list .agent-name")?.textContent === "builder-eu-1", "listing what the session watches");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]'), "the agents page stayed under it");

  await load("/settings", { ctx: c });
  assert.equal(document.querySelector('sf-s[data-sf-name="drawer"]')!.querySelector(".drawer"), null, "the drawer slot holds only its fallback");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/settings/page.tsx#default"][data-sf-mounted]'));
  await settle();
});

test("density is written optimistically from the drawer, read by the list in another root, and seeded back after the action", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  await fireEvent.click(screen.getByLabelText("Settings"));
  assert.equal(get(density), "comfortable");
  assert.equal(document.querySelector(".agent-rows-compact"), null);

  await fireEvent.click(screen.getByText("Compact"));
  assert.equal(get(density), "compact", "written before the action answered");
  assert.ok(document.querySelector(".agent-rows-compact"), "the list, in the agents layout's root, went compact");
  await settle();
  assert.equal(c.session.density, "compact", "the action held it in the session");
  assert.equal(get(density), "compact", "and the revalidation seeded the same value back");
  assert.ok(document.querySelector(".agent-rows-compact"));
  await settle();
});

test("unwatching from the drawer moves the header count before the server answers", async () => {
  const c = ctx({ session: { watching: { "1": true }, density: "comfortable" }, services: services() });
  await load("/settings", { ctx: c });
  assert.equal(get(watching), 1);
  await fireEvent.click(screen.getByText("unwatch"));
  assert.equal(get(watching), 0);
  await settle();
  assert.equal(c.session.watching, {});
  assert.ok(screen.getByLabelText("watching 0 agents"));
  await settle();
});
