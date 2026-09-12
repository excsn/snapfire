import { assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

test("planning a recipe writes the session and the count in the masthead moves with it", async () => {
  const c = ctx({
    session: { planned: {} },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/recipe/1", { ctx: c });
  const count = screen.getByLabelText("tonight");
  assert.equal(count.textContent, "0 for tonight");
  await fireEvent.click(count);
  assert.ok(screen.getByText(/Kept in the session cookie/), "the panel is open");

  await fireEvent.click(screen.getByText("Cook this tonight"));
  await settle();

  assert.equal(c.session.planned, { "1": true }, "the action wrote the session through the interpreter");
  assert.equal(screen.getByLabelText("tonight").textContent, "1 for tonight", "and the masthead island followed the store");
  assert.ok(screen.getByText("Planned for tonight"));
  assert.ok(screen.getByLabelText("tonight") === count, "the layout re-rendered around the island and kept its DOM");
  assert.ok(screen.getByText(/Kept in the session cookie/), "and the panel is still open");
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
  assert.equal(screen.getByText("serves 4").textContent, "serves 4");

  await fireEvent.click(screen.getByLabelText("more"));
  await fireEvent.click(screen.getByLabelText("more"));

  assert.ok(screen.getByText("serves 6"));
  assert.ok(screen.getByText("4.5"), "three leeks for four became four and a half for six");
  assert.ok(screen.getByText("1.5 l"));
  assert.ok(screen.getByText("scaled from 4"));
});
