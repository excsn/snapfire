import { load } from "@routes/tonight/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const pears = { ...soup, id: "6", title: "Poached pears", course: "Pudding", minutes: 40 };

test("tonight is the box filtered by the session, with the minutes added up", async () => {
  const c = ctx({ session: { planned: { "6": true, "1": true } }, services: { kitchen: { listRecipes: () => [soup, pears] } } });
  const { tonight, minutes } = await load(c);
  assert.equal(tonight, [soup, pears]);
  assert.equal(minutes, 75n);
});

test("an empty session keeps the one call and plans nothing", async () => {
  const c = ctx({ services: { kitchen: { listRecipes: () => [soup, pears] } } });
  const { tonight, minutes } = await load(c);
  assert.equal(tonight, []);
  assert.equal(minutes, 0n);
  assert.equal(c.trace.calls.length, 1);
});
