import { ctx, expect, fireEvent, render, settle, test } from "@snapfire/fsr-client/testing";

import PlanRecipe from "@src/ui/PlanRecipe.vue";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };
const kitchen = { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] };

test("a spec with no JSX renders a Vue island and hydrates it over the server's markup", async () => {
  const c = ctx({ session: { planned: {} }, services: { kitchen } });
  const r = await render({ type: PlanRecipe, props: { id: "1", planned: false } }, { ctx: c });
  expect(r.hydrated).toEqual("src/ui/PlanRecipe.vue#default");
  expect(r.container.querySelector("sf-i")?.hasAttribute("data-sf-mounted"), "Vue's adapter mounted it").toEqual(true);

  await fireEvent.click(r.getByText("Cook this tonight"));
  await settle();
  expect(c.session.planned, "the click ran the action").toEqual({ "1": true });
  expect(r.getByText("Planned for tonight")).toBeTruthy();
});

test("rerendering with a plain element patches the Vue island in place", async () => {
  const r = await render({ type: PlanRecipe, props: { id: "1", planned: false } });
  const button = r.getByText("Cook this tonight");
  await r.rerender({ type: PlanRecipe, props: { id: "2", planned: true } });
  expect(r.getByRole("button"), "the patch kept the element").toBe(button);
});
