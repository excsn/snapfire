import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const agents = [
  { id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 },
  { id: 3n, name: "builder-us-1", region: "us", status: "down", queue_depth: 7n, cpu: 0.5 },
];
const jobs = [{ id: 11n, name: "compile", seconds: 92n }];

function services() {
  return { fleet: { listAgents: () => agents, getAgent: ({ id }: { id: number }) => agents.find((a) => a.id === BigInt(id))!, listJobs: () => jobs, listAlerts: () => [] } };
}

const mark = (text: string) => screen.getByText(text).getAttribute("aria-current");

test("the nav is marked by the page beneath a drawer while the gear is marked by the address", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  expect(mark("Agents"), "a section link on its own page").toEqual("true");
  expect(mark("Help")).toBeNull();
  const gear = screen.getByLabelText("Settings");
  expect(gear.getAttribute("aria-current")).toBeNull();

  await fireEvent.click(gear);
  expect(location.pathname).toEqual("/settings");
  expect(document.querySelector('sf-i[data-sf-module="routes/settings/page.drawer.tsx#default"]'), "the drawer opened over the list").toBeTruthy();
  expect(mark("Agents"), "the nav describes the page beneath the drawer").toEqual("true");
  expect(gear.getAttribute("aria-current"), "the gear is judged by the address, which is its own").toEqual("page");

  await fireEvent.click(screen.getByText("Help"));
  expect(location.pathname).toEqual("/help");
  expect(mark("Help"), "a full navigation moves the mark").toEqual("page");
  expect(mark("Agents")).toBeNull();
  expect(gear.getAttribute("aria-current"), "and the gear lets go").toBeNull();
  await settle();
});

test("a peek keeps the section mark under either rule, since the agent is under the section", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  await fireEvent.click(screen.getAllByText("peek")[1]);
  expect(location.pathname).toEqual("/agents/3");
  expect(mark("Agents")).toEqual("true");
  await settle();
});
