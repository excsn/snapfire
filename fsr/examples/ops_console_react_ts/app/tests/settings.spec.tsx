import { get } from "@snapfire/fsr-client";
import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

import { density, watching } from "@src/store";

const agents = [{ id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 }];

function services() {
  return { fleet: { listAgents: () => agents, listAlerts: () => [] } };
}

test("the gear opens settings in the root layout's drawer, and a document load is the whole page", async () => {
  const c = ctx({ session: { watching: { "1": true }, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const drawer = document.querySelector('sf-s[data-sf-name="drawer"]')!;
  expect(drawer.querySelector(".drawer-hint")).toBeTruthy();

  await fireEvent.click(screen.getByLabelText("Settings"));
  expect(location.pathname).toEqual("/settings");
  expect(drawer.querySelector('sf-i[data-sf-module="routes/settings/page.drawer.tsx#default"][data-sf-mounted]'), "the drawer variant hydrated in the root layout's slot").toBeTruthy();
  expect(drawer.querySelector(".watch-list .agent-name")?.textContent, "listing what the session watches").toBe("builder-eu-1");
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]'), "the agents page stayed under it").toBeTruthy();

  await load("/settings", { ctx: c });
  expect(document.querySelector('sf-s[data-sf-name="drawer"]')!.querySelector(".drawer"), "the drawer slot holds only its fallback").toBeNull();
  expect(document.querySelector('sf-i[data-sf-module="routes/settings/page.tsx#default"][data-sf-mounted]')).toBeTruthy();
  await settle();
});

test("density is written optimistically from the drawer, read by the list in another root, and seeded back after the action", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  await fireEvent.click(screen.getByLabelText("Settings"));
  expect(get(density)).toEqual("comfortable");
  expect(document.querySelector(".agent-rows-compact")).toBeNull();

  await fireEvent.click(screen.getByText("Compact"));
  expect(get(density), "written before the action answered").toEqual("compact");
  expect(document.querySelector(".agent-rows-compact"), "the list, in the agents layout's root, went compact").toBeTruthy();
  await settle();
  expect(c.session.density, "the action held it in the session").toEqual("compact");
  expect(get(density), "and the revalidation seeded the same value back").toEqual("compact");
  expect(document.querySelector(".agent-rows-compact")).toBeTruthy();
  await settle();
});

test("unwatching from the drawer moves the header count before the server answers", async () => {
  const c = ctx({ session: { watching: { "1": true }, density: "comfortable" }, services: services() });
  await load("/settings", { ctx: c });
  expect(get(watching)).toEqual(1);
  await fireEvent.click(screen.getByText("unwatch"));
  expect(get(watching)).toEqual(0);
  await settle();
  expect(c.session.watching).toEqual({});
  expect(screen.getByLabelText("watching 0 agents")).toBeTruthy();
  await settle();
});
