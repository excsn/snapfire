import { assert, ctx, f64, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const agents = [
  { id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 },
  { id: 3n, name: "builder-us-1", region: "us", status: "down", queue_depth: 7n, cpu: f64(0) },
];
const alerts = [{ id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" }];
const jobs = [{ id: 11n, name: "compile", seconds: 92n }];

function services() {
  return { fleet: { listAgents: () => agents, getAgent: () => agents[0], listJobs: () => jobs, listAlerts: () => alerts } };
}

test("a full link renders the agent under the list, peek renders it beside the list, and the list stays", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const peek = document.querySelector('sf-s[data-sf-name="peek"]')!;
  assert.ok(peek.querySelector(".peek-hint"), "the peek slot shows its fallback");
  const list = document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]');
  assert.ok(list && screen.getByText("Pick an agent from the list."));

  await fireEvent.click(screen.getByText("builder-eu-1"));
  assert.equal(location.pathname, "/agents/view/1");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.tsx#default"][data-sf-mounted]'), "the page took the content slot under the list");
  assert.equal(screen.queryByText("Pick an agent from the list."), null);
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]') === list, "the list kept its DOM");
  assert.ok(peek.querySelector(".peek-hint"), "and the peek slot its fallback");

  await fireEvent.click(screen.getAllByText("peek")[1]);
  assert.equal(location.pathname, "/agents/view/3");
  assert.ok(peek.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.peek.tsx#default"][data-sf-mounted]'), "`into` picked the variant the nested layout declares");
  assert.equal(peek.querySelector(".peek-hint"), null);
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]') === list);
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.tsx#default"]'), "the page under the list stayed too");
  await settle();
});

test("a plain link from an alert peeks from the list and navigates from the summary", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents", { ctx: c });
  const peek = document.querySelector('sf-s[data-sf-name="peek"]')!;
  await settle();
  await fireEvent.click(screen.getByText("open"));
  assert.equal(location.pathname, "/agents/view/3");
  assert.ok(peek.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.peek.tsx#default"]'), "the origin shares the agents layout, so the server chose its slot");

  await load("/", { ctx: c });
  await settle();
  await fireEvent.click(screen.getByText("open"));
  assert.equal(location.pathname, "/agents/view/3");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.tsx#default"][data-sf-mounted]'), "no agents layout on the summary to intercept into, so the page rendered whole");
  assert.ok(document.querySelector('sf-s[data-sf-name="peek"] .peek-hint'), "under the agents layout the page brought with it, whose peek slot holds its fallback");
  assert.equal(document.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.peek.tsx#default"]'), null);
  await settle();
});

test("a region in the query survives both kinds of intercept", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents?region=eu", { ctx: c });
  assert.equal(document.querySelector(".pill-region")?.textContent, "eu");
  await fireEvent.click(screen.getAllByText("peek")[0]);
  assert.equal(location.pathname + location.search, "/agents/view/1?region=eu");
  assert.equal(document.querySelector(".pill-region")?.textContent, "eu", "the agents layout re-ran with the same query and kept the region");
  assert.equal(document.querySelector(".chip-on")?.textContent, "eu");
  await settle();
});

test("a document load of an agent streams the page and the alerts behind their fallbacks", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents/view/1", { ctx: c });
  await settle();
  assert.equal(document.title, "builder-eu-1 · Ops console");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/agents/view/[id]/page.tsx#default"][data-sf-mounted]'));
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/slots/alerts/page.tsx#default"][data-sf-mounted]'), "the parallel slot resolved and hydrated");
  assert.ok(document.querySelector('sf-s[data-sf-island][data-sf-when="visible"] sf-i'), "the job timeline is placed as its own island, timed on visibility");
  await settle();
});
