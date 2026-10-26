import { ctx, expect, fireEvent, load, screen, settle, spyOn, test } from "@snapfire/fsr/client/testing";

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

test("an island the layout's page places on visible hydrates the markup the server wrote", async () => {
  const reported = spyOn(console, "error");
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/two", { ctx: c });
  const written = screen.getByLabelText("page count");
  await settle();
  expect(reported, "hydration found what the server wrote").not.toHaveBeenCalled();
  expect(screen.getByLabelText("page count"), "the server's button was hydrated, not rendered again").toBe(written);
  await fireEvent.click(written);
  expect(written.textContent).toEqual("this page 1");
  expect(screen.getByLabelText("count").textContent, "the layout's count is its own").toEqual("clicked 0");
  reported.mockRestore();
});

test("an island on visible in a page the navigator writes into the layout hydrates the markup the server wrote", async () => {
  const reported = spyOn(console, "error");
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/one", { ctx: c });
  await settle();
  await fireEvent.click(screen.getByText("route two"));
  await settle();
  expect(location.pathname).toEqual("/state/two");
  expect(reported, "hydration found what the server wrote").not.toHaveBeenCalled();
  const button = screen.getByLabelText("page count");
  await fireEvent.click(button);
  expect(button.textContent).toEqual("this page 1");
  reported.mockRestore();
});

test("an island the layout places itself renders its own markup beside an island its page places", async () => {
  const reported = spyOn(console, "error");
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/two", { ctx: c });
  await settle();
  const page = document.querySelector(".state-page") as Element;
  expect(page.querySelector("button[aria-label='page count']"), "the page's island is in the page").toBeTruthy();
  expect(page.querySelector("details.tips"), "and the layout's is not").toBeNull();
  const tips = document.querySelectorAll("section.state details.tips");
  expect(tips.length, "the layout's island is there once").toEqual(1);
  expect(tips[0].closest(".state-page, .state-aside"), "outside the page and the slot").toBeNull();
  await fireEvent.click(screen.getByLabelText("page count"));
  expect(screen.getByLabelText("page count").textContent).toEqual("this page 1");
  expect(reported, "every root hydrated what the server wrote").not.toHaveBeenCalled();
  reported.mockRestore();
});

test("an island the layout's named slot places on visible hydrates the markup the server wrote", async () => {
  const reported = spyOn(console, "error");
  const c = ctx({ session: { watching: {} }, services: services() });
  await load("/state/one", { ctx: c });
  const written = screen.getByText("dismiss");
  await settle();
  expect(reported, "hydration found what the server wrote").not.toHaveBeenCalled();
  expect(screen.getByText("dismiss"), "the server's button was hydrated, not rendered again").toBe(written);
  await fireEvent.click(written);
  expect(document.querySelector(".state-aside .hint")?.textContent).toEqual("Note put away.");
  reported.mockRestore();
});
