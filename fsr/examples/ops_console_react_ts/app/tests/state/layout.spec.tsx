import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const alerts = [{ id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" }];

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => alerts } };
}

test("a count held in the layout survives a navigation between its routes", async () => {
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/one", { ctx: c });
  expect(screen.getByText("Route one")).toBeTruthy();

  const counter = screen.getByLabelText("count");
  await fireEvent.click(counter);
  await fireEvent.click(counter);
  expect(counter.textContent).toEqual("clicked 2");

  await fireEvent.click(screen.getByText("route two"));

  expect(location.pathname).toEqual("/state/two");
  expect(screen.getByText("Route two")).toBeTruthy();
  expect(screen.queryByText("Route one")).toBeNull();
  expect(screen.getByLabelText("count"), "the layout's DOM was kept").toBe(counter);
  expect(counter.textContent, "and its state with it").toEqual("clicked 2");
});
