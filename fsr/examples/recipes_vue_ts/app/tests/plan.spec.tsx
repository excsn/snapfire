import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

test("planning a recipe writes the session and the count in the masthead moves with it", async () => {
  const c = ctx({
    session: { planned: {} },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/recipe/1", { ctx: c });
  const count = screen.getByLabelText("tonight");
  expect(count.textContent).toEqual("0 for tonight");
  await fireEvent.click(count);
  expect(screen.getByText(/Kept in the session cookie/), "the panel is open").toBeTruthy();

  await fireEvent.click(screen.getByText("Cook this tonight"));
  await settle();

  expect(c.session.planned, "the action wrote the session through the interpreter").toEqual({ "1": true });
  expect(screen.getByLabelText("tonight").textContent, "and the masthead island followed the store").toEqual("1 for tonight");
  expect(screen.getByText("Planned for tonight")).toBeTruthy();
  expect(screen.getByLabelText("tonight"), "the layout re-rendered around the island and kept its DOM").toBe(count);
  expect(screen.getByText(/Kept in the session cookie/), "and the panel is still open").toBeTruthy();
});

test("the scaler multiplies every quantity from the serves it was given", async () => {
  const c = ctx({
    services: {
      kitchen: {
        getBox: () => box,
        listRecipes: () => [{ ...soup, ingredients: [{ name: "leeks", quantity: 3, unit: "" }, { name: "stock", quantity: 1, unit: "l" }] }],
        listNotes: () => [],
        listMarket: () => [],
      },
    },
  });
  await load("/recipe/1", { ctx: c });
  expect(screen.getByText("serves 4").textContent).toEqual("serves 4");

  await fireEvent.click(screen.getByLabelText("more"));
  await fireEvent.click(screen.getByLabelText("more"));

  expect(screen.getByText("serves 6")).toBeTruthy();
  expect(screen.getByText("4.5"), "three leeks for four became four and a half for six").toBeTruthy();
  expect(screen.getByText("1.5 l")).toBeTruthy();
  expect(screen.getByText("scaled from 4")).toBeTruthy();
});
