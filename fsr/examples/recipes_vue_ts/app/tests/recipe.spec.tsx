import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

const soup = {
  id: "1",
  title: "Leek and potato soup",
  course: "Starter",
  cook: "Ama",
  minutes: 35,
  serves: 4,
  ingredients: [{ name: "leeks", quantity: 3, unit: "" }],
  method: "Soften the leeks.",
};
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

const kitchen = (planned: Record<string, boolean> = {}) =>
  ctx({
    session: { planned },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });

test("a recipe page is the recipe under its own layout under the masthead", async () => {
  await load("/recipe/1", { ctx: kitchen() });
  assert.equal(document.querySelector(".recipe h2")?.textContent, "Leek and potato soup");
  assert.ok(document.querySelector(".crumbs"), "the recipe layout is between the masthead and the page");
  assert.ok(screen.getByText("Soften the leeks."));
  assert.equal(document.title, "Leek and potato soup · The Sunday Box", "the title came from the loader's data");
});

test("an id the box does not hold renders the segment's own boundary", async () => {
  await load("/recipe/99", { ctx: kitchen() });
  assert.ok(screen.getByText("Not in the box"));
  assert.equal(document.querySelector(".masthead h1")?.textContent, "The Sunday Box", "the layout above it still rendered");
});

test("a Vue island is placed empty by the server and carries its props for the mount", async () => {
  await load("/recipe/1", { ctx: kitchen({ "1": true }) });
  const islands = Array.from(document.querySelectorAll(".recipe sf-i[data-sf-module$='.vue#default']"));
  assert.equal(islands.length, 2, "the plan control and the scaler; the masthead's is the layout's");
  const plan = islands.find((el) => el.getAttribute("data-sf-module")?.endsWith("PlanRecipe.vue#default"));
  assert.ok(plan, "the plan control is a Vue island");
  const props = JSON.parse(document.querySelector(`script[data-sf-props="${plan?.id}"]`)?.textContent ?? "{}");
  assert.equal(props.id, "1");
  assert.equal(props.planned, true);
});
