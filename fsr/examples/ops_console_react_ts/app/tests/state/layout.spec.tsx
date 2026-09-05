import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const alerts = [{ id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" }];

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => alerts } };
}

test("a count held in the layout survives a navigation between its routes", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/one", { ctx: c });
  assert.ok(screen.getByText("Route one"));

  const counter = screen.getByLabelText("count");
  await fireEvent.click(counter);
  await fireEvent.click(counter);
  assert.equal(counter.textContent, "clicked 2");

  await fireEvent.click(screen.getByText("route two"));

  assert.equal(location.pathname, "/state/two");
  assert.ok(screen.getByText("Route two"));
  assert.equal(screen.queryByText("Route one"), null);
  assert.ok(screen.getByLabelText("count") === counter, "the layout's DOM was kept");
  assert.equal(counter.textContent, "clicked 2", "and its state with it");
});
