import { load } from "@routes/recipe/[id]/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const beetroot = { ...soup, id: "2", title: "Beetroot with goat's curd", cook: "Tomas" };
const chicken = { ...soup, id: "3", title: "Roast chicken with bread sauce", course: "Main" };

test("the recipe is picked out of the box and the rest of its course comes with it", async () => {
  const c = ctx<void, "/recipe/{id}">({ params: { id: "1" }, services: { kitchen: { listRecipes: () => [soup, beetroot, chicken] } } });
  const { recipe, sameCourse, planned } = await load(c);
  assert.equal(recipe.title, "Leek and potato soup");
  assert.equal(sameCourse, [beetroot], "the other starter and not the main");
  assert.equal(planned, false);
});

test("an id the box does not hold is refused as not_found", async () => {
  const c = ctx<void, "/recipe/{id}">({ params: { id: "99" }, services: { kitchen: { listRecipes: () => [soup, beetroot, chicken] } } });
  await assert.rejects(load(c), "not_found");
});

test("a recipe already planned says so, from the session alone", async () => {
  const c = ctx<void, "/recipe/{id}">({ params: { id: "3" }, session: { planned: { "3": true } }, services: { kitchen: { listRecipes: () => [soup, beetroot, chicken] } } });
  const { planned } = await load(c);
  assert.equal(planned, true);
});
