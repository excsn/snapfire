import { createEvent, ctx, expect, load, settle, test } from "@snapfire/fsr-client/testing";

const soup = { id: "1", title: "Leek and potato soup", course: "Starter", cook: "Ama", minutes: 35, serves: 4, ingredients: [], method: "" };
const box = { name: "The Sunday Box", tagline: "Six recipes", courses: ["Starter"] };

const kitchen = () =>
  ctx({
    services: { kitchen: { getBox: () => box, listRecipes: () => [soup], listNotes: () => [], listMarket: () => [] } },
  });

/** Clicks `link` without settling, so the navigation it starts is still in flight when the caller looks. */
async function startFollowing(link: Element): Promise<void> {
  link.dispatchEvent(createEvent("click", link));
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

test("a Vue island the server placed in a link reads that link's navigation as pending", async () => {
  await load("/", { ctx: kitchen() });
  await settle();
  const link = document.querySelector('.masthead-nav a[href="/tonight"]');
  expect(link?.querySelector(".opening"), "the link holds the island").toBeTruthy();
  await startFollowing(link as Element);
  expect(link?.hasAttribute("data-sf-pending"), "the link is marked while the payload is fetched").toBe(true);
  expect(link?.querySelector(".opening-on")?.textContent, "and the island inside it says so").toEqual("opening…");
  await settle();
  expect(location.pathname).toEqual("/tonight");
  expect(document.querySelector("[data-sf-pending]"), "the mark is gone once the page is shown").toBeNull();
});
