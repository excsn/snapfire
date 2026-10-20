import { ctx, expect, fireEvent, render, settle, test } from "@snapfire/fsr-client/testing";

import PlanRecipe from "@src/ui/PlanRecipe.vue";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };
const kitchen = { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] };

/** The masthead's `notices` place, which the control's notice teleports into; the layout declares it on a page. */
function notices(): void {
  if (document.getElementById("notices")) return;
  const place = document.createElement("div");
  place.id = "notices";
  document.body.append(place);
}

test("a Vue island renders on its own and hydrates over the server's markup", async () => {
  notices();
  const c = ctx({ session: { planned: {} }, services: { kitchen } });
  const r = await render(<PlanRecipe id="1" planned={false} />, { ctx: c });
  expect(r.hydrated).toEqual("src/ui/PlanRecipe.vue#default");
  expect(r.container.querySelector("sf-i")?.hasAttribute("data-sf-mounted"), "Vue's adapter mounted it").toEqual(true);

  await fireEvent.click(r.getByText("Cook this tonight"));
  await settle();
  expect(c.session.planned, "the click ran the action").toEqual({ "1": true });
  expect(r.getByText("Planned for tonight")).toBeTruthy();
});

test("rerendering a Vue island patches it in place", async () => {
  notices();
  const r = await render(<PlanRecipe id="1" planned={false} />);
  const button = r.getByText("Cook this tonight");
  await r.rerender(<PlanRecipe id="2" planned={true} />);
  expect(r.getByRole("button"), "the patch kept the element").toBe(button);
});
