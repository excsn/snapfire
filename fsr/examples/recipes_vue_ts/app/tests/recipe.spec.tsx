import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

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
  expect(document.querySelector(".recipe h2")?.textContent).toEqual("Leek and potato soup");
  expect(document.querySelector(".crumbs"), "the recipe layout is between the masthead and the page").toBeTruthy();
  expect(screen.getByText("Soften the leeks.")).toBeTruthy();
  expect(document.title, "the title came from the loader's data").toEqual("Leek and potato soup · The Sunday Box");
});

test("an id the box does not hold renders the segment's own boundary", async () => {
  await load("/recipe/99", { ctx: kitchen() });
  expect(screen.getByText("Not in the box")).toBeTruthy();
  expect(document.querySelector(".masthead h1")?.textContent, "the layout above it still rendered").toEqual("The Sunday Box");
});

test("a Vue island carries its props beside its markup for the mount", async () => {
  await load("/recipe/1", { ctx: kitchen({ "1": true }) });
  const islands = Array.from(document.querySelectorAll(".recipe sf-i[data-sf-module$='.vue#default']"));
  expect(islands.length, "the plan control and the scaler; the masthead's is the layout's").toEqual(2);
  const plan = islands.find((el) => el.getAttribute("data-sf-module")?.endsWith("PlanRecipe.vue#default"));
  expect(plan, "the plan control is a Vue island").toBeTruthy();
  const props = JSON.parse(document.querySelector(`script[data-sf-props="${plan?.id}"]`)?.textContent ?? "{}");
  expect(props.id).toEqual("1");
  expect(props.planned).toEqual(true);
});
