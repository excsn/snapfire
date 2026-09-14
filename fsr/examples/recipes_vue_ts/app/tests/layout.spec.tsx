import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

const kitchen = () =>
  ctx({
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });

test("the panel opened in the masthead is still open after the page beneath it changes", async () => {
  await load("/", { ctx: kitchen() });
  const count = screen.getByLabelText("tonight");
  await fireEvent.click(count);
  expect(screen.getByText(/Kept in the session cookie/), "the panel is open").toBeTruthy();

  await fireEvent.click(screen.getByText("Leek and potato soup"));

  expect(location.pathname).toEqual("/recipe/1");
  expect(document.querySelector(".recipe h2"), "the page region was replaced").toBeTruthy();
  expect(screen.getByLabelText("tonight"), "the layout's DOM was kept").toBe(count);
  expect(screen.getByText(/Kept in the session cookie/), "and the state inside it").toBeTruthy();
});

test("the markup the layout writes inside a Vue island is the island's slot", async () => {
  await load("/", { ctx: kitchen() });
  expect(document.querySelector(".tonight-note"), "the panel starts closed").toBeNull();
  await fireEvent.click(screen.getByLabelText("tonight"));
  const region = document.querySelector(".tonight-note sf-s[data-sf-children]");
  expect(region, "the slot is the children region").toBeTruthy();
  expect(region?.textContent ?? "").toMatch(/Kept in the session cookie/);
  expect(region?.querySelector("a")?.getAttribute("href")).toEqual("/tonight");
});

test("the tonight count is seeded by the layout loader and read through the store", async () => {
  const held = ctx({
    session: { planned: { "1": true } },
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });
  await load("/", { ctx: held });
  expect(screen.getByLabelText("tonight").textContent).toEqual("1 for tonight");
});

test("an element the Vue island creates after a second load belongs to that page's document and takes focus", async () => {
  await load("/", { ctx: kitchen() });
  await load("/", { ctx: kitchen() });
  const count = screen.getByLabelText("tonight");
  expect(count.ownerDocument).toBe(document);
  count.focus();
  expect(document.activeElement).toBe(count);
});

test("a document held from an earlier page finds the current page's elements", async () => {
  await load("/", { ctx: kitchen() });
  const held = document;
  await load("/", { ctx: kitchen() });
  expect(held).not.toBe(document);
  expect(held.querySelector("button[aria-label=tonight]")).toBe(screen.getByLabelText("tonight"));
  expect(held.body).toBe(document.body);
});
