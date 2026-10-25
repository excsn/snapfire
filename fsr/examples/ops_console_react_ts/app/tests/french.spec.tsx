import { get } from "@snapfire/fsr/client";
import { ctx, expect, f64, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

import { headline, watching } from "@src/store";

const agents = [
  { id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 },
  { id: 3n, name: "builder-us-1", region: "us", status: "down", queue_depth: 7n, cpu: f64(0) },
];
const alerts = [
  { id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" },
  { id: 22n, agent_id: 1n, level: "warn", text: "queue over 3" },
];

function services() {
  return { fleet: { listAgents: () => agents, getAgent: ({ id }: { id: number }) => agents.find((a) => a.id === BigInt(id))!, getVisits: () => ({ count: 0n }), listJobs: () => [], listAlerts: () => alerts, acknowledgeAlert: () => [alerts[1]] } };
}

test("under /fr_FR/ the server paints the header, the list and the derived headline in French", async () => {
  const c = ctx({ session: { watching: { "1": true } }, services: services() });
  await load("/fr_FR/agents", { ctx: c });

  expect(get(headline)).toEqual("2 à examiner, 1 suivi");
  expect(screen.getByText("2 à examiner, 1 suivi")).toBeTruthy();
  expect(screen.getByLabelText("2 alertes ouvertes")).toBeTruthy();
  expect(screen.getByLabelText("1 agent suivi")).toBeTruthy();
  expect(screen.getByText("ne plus suivre")).toBeTruthy();
  expect(screen.getByText("2 affichés")).toBeTruthy();
  await settle();
  await load("/fr_FR/agents/1", { ctx: c });
  expect(document.title, "the page's meta reads the French catalog").toEqual("builder-eu-1 · Console ops");
  await settle();
});

test("unwatching under /fr_FR/ recomputes the headline in French in the browser", async () => {
  const c = ctx({ session: { watching: { "1": true } }, services: services() });
  await load("/fr_FR/agents", { ctx: c });

  await fireEvent.click(screen.getByText("ne plus suivre"));
  expect(get(watching)).toEqual(0);
  expect(get(headline), "the browser's derive reads the document's locale").toEqual("2 à examiner");
  await settle();
  expect(screen.getAllByText("suivre").length).toEqual(2);
  expect(screen.getByLabelText("0 agent suivi")).toBeTruthy();
});

test("an agent's page offers whichever of watch and unwatch applies, and the revalidation flips it", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/fr_FR/agents/1", { ctx: c });

  await fireEvent.click(screen.getByText("Suivre cet agent"));
  await settle();
  expect(c.session.watching).toEqual({ "1": true });
  expect(screen.getByText("Ne plus suivre cet agent")).toBeTruthy();
  await fireEvent.click(screen.getByText("Ne plus suivre cet agent"));
  await settle();
  expect(c.session.watching).toEqual({});
  expect(screen.getByText("Suivre cet agent")).toBeTruthy();
  expect(get(headline)).toEqual("2 à examiner");
});
