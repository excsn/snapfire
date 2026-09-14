import { load } from "@routes/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter", "Main"] };
const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const chicken = { ...soup, id: "3", title: "Roast chicken with bread sauce", course: "Main", minutes: 90, serves: 6 };

test("the box is every recipe when no course is asked for", async () => {
  const c = ctx({ services: { kitchen: { listRecipes: () => [soup, chicken], getBox: () => box } } });
  const { course, courses, recipes } = await load(c);
  expect(course).toEqual("all");
  expect(courses).toEqual(["Starter", "Main"]);
  expect(recipes.length).toEqual(2);
});

test("a course in the query keeps only that course", async () => {
  const c = ctx({ query: { course: "Main" }, services: { kitchen: { listRecipes: () => [soup, chicken], getBox: () => box } } });
  const { recipes } = await load(c);
  expect(recipes).toEqual([chicken]);
});

test("the two calls leave together", async () => {
  const c = ctx({ services: { kitchen: { listRecipes: () => [soup], getBox: () => box } } });
  await load(c);
  expect(c.trace.calls.length).toEqual(2);
});

test("what is planned tonight comes out of the session as ids", async () => {
  const c = ctx({ session: { planned: { "3": true } }, services: { kitchen: { listRecipes: () => [soup, chicken], getBox: () => box } } });
  const { planned } = await load(c);
  expect(planned).toEqual(["3"]);
});
