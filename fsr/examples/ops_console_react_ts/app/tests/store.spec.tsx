import { get } from "@snapfire/fsr-client";
import { ctx, expect, f64, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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
  return { fleet: { listAgents: () => agents, getAgent: ({ id }: { id: number }) => agents.find((a) => a.id === BigInt(id))!, listJobs: () => jobs, listAlerts: () => alerts, acknowledgeAlert: () => [alerts[1]] } };
}

test("two layouts seed the store, the inner one wins the region, and the derived headline follows both", async () => {
  const c = ctx({ session: { watching: { "1": true } }, services: services() });
  await load("/agents?region=eu", { ctx: c });

  expect(get(openAlerts)).toEqual(2);
  expect(get(watching)).toEqual(1);
  expect(get(region), "the agents layout's seed replaced the root layout's `all`").toEqual("eu");
  expect(get(headline), "seeded by the server; the browser derives it again from src/main.ts, which the runner does not load").toEqual("2 to look at, watching 1");
  expect(screen.getByLabelText("2 open alerts")).toBeTruthy();
  expect(screen.getByText("2 to look at, watching 1")).toBeTruthy();
  await settle();
});

test("a key nothing seeds is written by the list and read by the header in another root", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/agents", { ctx: c });

  expect(get(selected)).toEqual(undefined);
  expect(document.querySelectorAll(".pill-selected").length).toEqual(0);
  await fireEvent.click(screen.getByText("builder-us-1"));
  expect(get(selected)).toEqual("3");
  expect(screen.getByText("#3"), "the header's pill appeared").toBeTruthy();
  expect(document.querySelectorAll(".agent-row-on").length, "and the list marked the row").toEqual(1);
  await settle();
});

test("watching an agent is optimistic in the header and the revalidation keeps it", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/agents", { ctx: c });

  expect(screen.getByLabelText("watching 0 agents")).toBeTruthy();
  await fireEvent.click(screen.getAllByText("watch")[0]);
  expect(get(watching), "written before the action answered").toEqual(1);
  await settle();
  expect(c.session.watching).toEqual({ "1": true });
  expect(screen.getByLabelText("watching 1 agents")).toBeTruthy();
  expect(get(headline)).toEqual("2 to look at, watching 1");
  await settle();
});
