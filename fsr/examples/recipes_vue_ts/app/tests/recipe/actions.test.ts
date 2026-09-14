import { plan, unplan } from "@routes/recipe/[id]/actions";
import { ctx, expect, test } from "@snapfire/fsr/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };

test("planning a recipe in the box writes the session and answers the count", async () => {
  const c = ctx<{ recipe_id: string }>({ input: { recipe_id: "1" }, session: { planned: {} }, services: { kitchen: { listRecipes: () => [soup] } } });
  const r = await plan(c);
  expect(r.planned).toEqual(1);
  expect(c.session.planned).toEqual({ "1": true });
});

test("planning one the box does not hold is refused and the session is untouched", async () => {
  const c = ctx<{ recipe_id: string }>({ input: { recipe_id: "9" }, session: { planned: {} }, services: { kitchen: { listRecipes: () => [soup] } } });
  await expect(plan(c)).rejects.toMatchObject({ kind: "not_found" });
  expect(c.session.planned).toEqual({});
});

test("unplanning takes it back out and leaves the others", async () => {
  const c = ctx<{ recipe_id: string }>({ input: { recipe_id: "1" }, session: { planned: { "1": true, "6": true } } });
  const r = await unplan(c);
  expect(r.planned).toEqual(1);
  expect(c.session.planned).toEqual({ "6": true });
});

test("unplanning one that was never planned is refused", async () => {
  const c = ctx<{ recipe_id: string }>({ input: { recipe_id: "1" }, session: { planned: {} } });
  await expect(unplan(c)).rejects.toMatchObject({ kind: "not_found" });
});
