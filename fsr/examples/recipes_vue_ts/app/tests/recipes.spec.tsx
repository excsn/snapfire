import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

const recipes = [
  { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" },
  { id: "3", title: "Roast chicken with bread sauce", course: "Main", cook: "Ama", minutes: 90, serves: 6, ingredients: [], method: "" },
];

const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter", "Main"] };

const kitchen = () =>
  ctx({
    services: {
      kitchen: {
        getBox: () => box,
        listRecipes: () => recipes,
        listNotes: () => [{ day: "Monday", text: "The oven runs hot." }],
        listMarket: () => {
          throw new Error("the market board is not answering");
        },
      },
    },
  });

test("the box is a row per recipe under the masthead the layout loaded", async () => {
  await load("/", { ctx: kitchen() });
  assert.equal(document.querySelector(".masthead h1")?.textContent, "The Sunday Box");
  const rows = Array.from(document.querySelectorAll(".recipe-row"));
  assert.equal(rows.length, 2, "one row per recipe");
  assert.equal(rows[0]?.querySelector(".recipe-title")?.textContent, "Leek and potato soup");
  assert.equal(rows[1]?.querySelector(".course")?.textContent, "Main");
});

test("one slot answers while the other is down and the page is whole either way", async () => {
  await load("/", { ctx: kitchen() });
  assert.ok(screen.getByText("The oven runs hot."), "the notes slot filled");
  assert.ok(document.querySelector(".market.panel-down"), "the market slot fell back to its own error boundary");
  assert.equal(document.querySelectorAll(".recipe-row").length, 2, "and the page beside it is untouched");
  assert.equal(document.querySelectorAll(".skeleton").length, 0, "both slots settled, so no fallback is left");
});

test("a course in the query narrows the rows and marks the chip", async () => {
  await load("/?course=Main", { ctx: kitchen() });
  assert.equal(document.querySelectorAll(".recipe-row").length, 1);
  assert.equal(document.querySelector(".chip-on")?.textContent, "Main");
});
