import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

const kitchen = () =>
  ctx({
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });

test("the panel opened in the masthead is still open after the page beneath it changes", async () => {
  await load("/", { ctx: kitchen() });
  const count = screen.getByLabelText("tonight");
  await fireEvent.click(count);
  assert.ok(screen.getByText(/Kept in the session cookie/), "the panel is open");

  await fireEvent.click(screen.getByText("Leek and potato soup"));

  assert.equal(location.pathname, "/recipe/1");
  assert.ok(document.querySelector(".recipe h2"), "the page region was replaced");
  assert.ok(screen.getByLabelText("tonight") === count, "the layout's DOM was kept");
  assert.ok(screen.getByText(/Kept in the session cookie/), "and the state inside it");
});

test("the tonight count is seeded by the layout loader and read through the store", async () => {
  const held = ctx({
    session: { planned: { "1": true } },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/", { ctx: held });
  assert.equal(screen.getByLabelText("tonight").textContent, "1 for tonight");
});
