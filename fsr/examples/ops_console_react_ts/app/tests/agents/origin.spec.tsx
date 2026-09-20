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

test("a peek opened by a link that does not carry the region keeps the region bar on the origin's filter", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/agents?region=eu", { ctx: c });
  expect(document.querySelector(".chip-on")?.textContent).toEqual("eu");
  const peek = screen.getAllByText("peek")[0];
  peek.setAttribute("href", "/agents/1");
  await fireEvent.click(peek);
  expect(location.pathname + location.search).toEqual("/agents/1");
  expect(document.querySelector(".peek h3")?.textContent, "the peek opened").toEqual("builder-eu-1");
  const island = document.querySelector('sf-i[data-sf-module="routes/agents/layout.tsx#default"]')!;
  const props = document.querySelector(`script[data-sf-props="${island.id}"]`)?.textContent ?? "";
  expect(props.includes('"eu"'), `the kept layout's props still say the origin's region: ${props}`).toBeTruthy();
  expect(document.querySelector(".chip-on")?.textContent, "the region bar still shows the filter the page underneath was loaded with").toEqual("eu");
  await settle();
});
