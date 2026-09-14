import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

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
  expect(document.querySelector(".masthead h1")?.textContent).toEqual("The Sunday Box");
  const rows = Array.from(document.querySelectorAll(".recipe-row"));
  expect(rows.length, "one row per recipe").toEqual(2);
  expect(rows[0]?.querySelector(".recipe-title")?.textContent).toEqual("Leek and potato soup");
  expect(rows[1]?.querySelector(".course")?.textContent).toEqual("Main");
});

test("one slot answers while the other is down and the page is whole either way", async () => {
  await load("/", { ctx: kitchen() });
  expect(screen.getByText("The oven runs hot."), "the notes slot filled").toBeTruthy();
  expect(document.querySelector(".market.panel-down"), "the market slot fell back to its own error boundary").toBeTruthy();
  expect(document.querySelectorAll(".recipe-row").length, "and the page beside it is untouched").toEqual(2);
  expect(document.querySelectorAll(".skeleton").length, "both slots settled, so no fallback is left").toEqual(0);
});

test("a course in the query narrows the rows and marks the chip", async () => {
  await load("/?course=Main", { ctx: kitchen() });
  expect(document.querySelectorAll(".recipe-row").length).toEqual(1);
  expect(document.querySelector(".chip-on")?.textContent).toEqual("Main");
});
