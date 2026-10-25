import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

test("planning a recipe writes the session and the count in the masthead moves with it", async () => {
  const c = ctx({
    session: { planned: {} },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/recipe/1", { ctx: c });
  const count = screen.getByLabelText("tonight");
  expect(count.textContent).toEqual("0 for tonight");
  await fireEvent.click(count);
  expect(screen.getByText(/Kept in the session cookie/), "the panel is open").toBeTruthy();

  await fireEvent.click(screen.getByText("Cook this tonight"));
  await settle();

  expect(c.session.planned, "the action wrote the session through the interpreter").toEqual({ "1": true });
  expect(screen.getByLabelText("tonight").textContent, "and the masthead island followed the store").toEqual("1 for tonight");
  expect(screen.getByText("Planned for tonight")).toBeTruthy();
  expect(screen.getByLabelText("tonight"), "the layout re-rendered around the island and kept its DOM").toBe(count);
  expect(screen.getByText(/Kept in the session cookie/), "and the panel is still open").toBeTruthy();
});

test("the scaler multiplies every quantity from the serves it was given", async () => {
  const c = ctx({
    services: {
      kitchen: {
        getBox: () => box,
        listRecipes: () => [{ ...soup, ingredients: [{ name: "leeks", quantity: 3, unit: "" }, { name: "stock", quantity: 1, unit: "l" }] }],
        listNotes: () => [],
        listMarket: () => [],
      },
    },
  });
  await load("/recipe/1", { ctx: c });
  expect(screen.getByText("serves 4").textContent).toEqual("serves 4");

  await fireEvent.click(screen.getByLabelText("more"));
  await fireEvent.click(screen.getByLabelText("more"));

  expect(screen.getByText("serves 6")).toBeTruthy();
  expect(screen.getByText("4.5"), "three leeks for four became four and a half for six").toBeTruthy();
  expect(screen.getByText("1.5 l")).toBeTruthy();
  expect(screen.getByText("scaled from 4")).toBeTruthy();
});

test("the plan control's notice is written in the masthead's place and follows the plan", async () => {
  const c = ctx({
    session: { planned: { "1": true } },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/recipe/1", { ctx: c });
  const notice = document.querySelector("#notices .notice");
  expect(notice?.textContent, "the server wrote it inside the layout's place").toEqual("On tonight's list");
  expect(notice?.closest(".masthead"), "which is in the masthead, far from the page that placed it").toBeTruthy();

  await fireEvent.click(screen.getByText("Planned for tonight"));
  await settle();
  expect(document.querySelector("#notices .notice"), "Vue hydrated the server's notice and took it away").toBeNull();

  await fireEvent.click(screen.getByText("Cook this tonight"));
  await settle();
  expect(document.querySelectorAll("#notices .notice").length, "and put one back in the same place").toEqual(1);

  await fireEvent.click(screen.getByText("Cook mode"));
  await settle();
  expect(document.querySelector("dialog.cook")?.hasAttribute("open"), "the body's dialog still answers after the plan revalidated the page").toEqual(true);
});

test("a navigation to another recipe hands the place to the new page's notice", async () => {
  const two = { ...soup, id: "2", title: "Fish pie" };
  const c = ctx({
    session: { planned: { "1": true } },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup, two], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/recipe/1", { ctx: c });
  expect(document.querySelector("#notices .notice")?.textContent).toEqual("On tonight's list");

  await fireEvent.click(document.querySelector('a[href="/recipe/2"]') as Element);
  await settle();
  expect(window.location.pathname).toEqual("/recipe/2");
  expect(document.querySelectorAll("#notices .notice").length, "recipe 1's notice left with its page").toEqual(0);

  await fireEvent.click(screen.getByText("Cook this tonight"));
  await settle();
  expect(document.querySelectorAll("#notices .notice").length, "recipe 2's island hydrated its own run and fills it").toEqual(1);
});
