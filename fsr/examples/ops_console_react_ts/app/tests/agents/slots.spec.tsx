import { ctx, expect, f64, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const agents = [
  { id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 },
  { id: 3n, name: "builder-us-1", region: "us", status: "down", queue_depth: 7n, cpu: f64(0) },
];
const alerts = [{ id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" }];
const jobs = [{ id: 11n, name: "compile", seconds: 92n }];

function services() {
  return { fleet: { listAgents: () => agents, getAgent: ({ id }: { id: number }) => agents.find((a) => a.id === BigInt(id))!, listJobs: () => jobs, listAlerts: () => alerts } };
}

test("a full link renders the agent under the list, peek renders it beside the list, and the list stays", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const peek = document.querySelector('sf-s[data-sf-name="peek"]')!;
  expect(peek.querySelector(".peek-hint"), "the peek slot shows its fallback").toBeTruthy();
  const list = document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]');
  expect(list && screen.getByText("Pick an agent from the list.")).toBeTruthy();

  await fireEvent.click(screen.getByText("builder-eu-1"));
  expect(location.pathname).toEqual("/agents/1");
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/[id]/page.tsx#default"][data-sf-mounted]'), "the page took the content slot under the list").toBeTruthy();
  expect(screen.queryByText("Pick an agent from the list.")).toBeNull();
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]'), "the list kept its DOM").toBe(list);
  expect(peek.querySelector(".peek-hint"), "and the peek slot its fallback").toBeTruthy();

  await fireEvent.click(screen.getAllByText("peek")[1]);
  expect(location.pathname).toEqual("/agents/3");
  expect(peek.querySelector(".peek h3")?.textContent, "`into` picked the variant the nested layout declares").toEqual("builder-us-1");
  expect(peek.querySelector(".peek-hint")).toBeNull();
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]')).toBe(list);
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/[id]/page.tsx#default"]'), "the page under the list stayed too").toBeTruthy();
  await settle();
});

test("a plain link from an alert peeks from the list and navigates from the summary", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const peek = document.querySelector('sf-s[data-sf-name="peek"]')!;
  await settle();
  await fireEvent.click(screen.getByText("open"));
  expect(location.pathname).toEqual("/agents/3");
  expect(peek.querySelector(".peek h3")?.textContent, "the origin shares the agents layout, so the server chose its slot").toEqual("builder-us-1");

  await load("/", { ctx: c });
  await settle();
  await fireEvent.click(screen.getByText("open"));
  expect(location.pathname).toEqual("/agents/3");
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/[id]/page.tsx#default"][data-sf-mounted]'), "no agents layout on the summary to intercept into, so the page rendered whole").toBeTruthy();
  expect(document.querySelector('sf-s[data-sf-name="peek"] .peek-hint'), "under the agents layout the page brought with it, whose peek slot holds its fallback").toBeTruthy();
  expect(document.querySelector('sf-s[data-sf-name="peek"] .peek')).toBeNull();
  await settle();
});

test("a region in the query survives both kinds of intercept", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents?region=eu", { ctx: c });
  expect(document.querySelector(".pill-region")?.textContent).toEqual("eu");
  await fireEvent.click(screen.getAllByText("peek")[0]);
  expect(location.pathname + location.search).toEqual("/agents/1?region=eu");
  expect(document.querySelector(".pill-region")?.textContent, "the agents layout re-ran with the same query and kept the region").toEqual("eu");
  expect(document.querySelector(".chip-on")?.textContent).toEqual("eu");
  await settle();
});

test("a document load of an agent streams the page and the alerts behind their fallbacks", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents/1", { ctx: c });
  await settle();
  expect(document.title).toEqual("builder-eu-1 · Ops console");
  expect(document.querySelector('sf-i[data-sf-module="routes/agents/[id]/page.tsx#default"][data-sf-mounted]')).toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/slots/alerts/page.tsx#default"][data-sf-mounted]'), "the parallel slot resolved and hydrated").toBeTruthy();
  expect(document.querySelector('sf-s[data-sf-island][data-sf-when="visible"] sf-i'), "the job timeline is placed as its own island, timed on visibility").toBeTruthy();
  await settle();
});
