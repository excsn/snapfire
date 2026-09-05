import { get } from "@snapfire/fsr-client";
import { assert, ctx, f64, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

import { headline, openAlerts, region, selected, watching } from "@src/store";

const agents = [
  { id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 },
  { id: 3n, name: "builder-us-1", region: "us", status: "down", queue_depth: 7n, cpu: f64(0) },
];
const alerts = [
  { id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" },
  { id: 22n, agent_id: 1n, level: "warn", text: "queue over 3" },
];
const jobs = [{ id: 11n, name: "compile", seconds: 92n }];

function services() {
  return { fleet: { listAgents: () => agents, getAgent: () => agents[0], listJobs: () => jobs, listAlerts: () => alerts, acknowledgeAlert: () => [alerts[1]] } };
}

test("two layouts seed the store, the inner one wins the region, and the derived headline follows both", async () => {
  const c = ctx({ session: { watching: { "1": true } }, services: services() });
  await load("/agents?region=eu", { ctx: c });

  assert.equal(get(openAlerts), 2);
  assert.equal(get(watching), 1);
  assert.equal(get(region), "eu", "the agents layout's seed replaced the root layout's `all`");
  assert.equal(get(headline), "2 to look at, watching 1", "seeded by the server; the browser derives it again from src/main.ts, which the runner does not load");
  assert.ok(screen.getByLabelText("2 open alerts"));
  assert.ok(screen.getByText("2 to look at, watching 1"));
  await settle();
});

test("a key nothing seeds is written by the list and read by the header in another root", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/agents", { ctx: c });

  assert.equal(get(selected), undefined);
  assert.equal(document.querySelectorAll(".pill-selected").length, 0);
  await fireEvent.click(screen.getByText("builder-us-1"));
  assert.equal(get(selected), "3");
  assert.ok(screen.getByText("#3"), "the header's pill appeared");
  assert.equal(document.querySelectorAll(".agent-row-on").length, 1, "and the list marked the row");
  await settle();
});

test("watching an agent is optimistic in the header and the revalidation keeps it", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/agents", { ctx: c });

  assert.ok(screen.getByLabelText("watching 0 agents"));
  await fireEvent.click(screen.getAllByText("watch")[0]);
  assert.equal(get(watching), 1, "written before the action answered");
  await settle();
  assert.equal(c.session.watching, { "1": true });
  assert.ok(screen.getByLabelText("watching 1 agents"));
  assert.equal(get(headline), "2 to look at, watching 1");
  await settle();
});
