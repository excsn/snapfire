import { createEvent, ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "What you get back when the thing your server reads is data." },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "09:30", ends: "10:10", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

const TALK_ISLAND = 'sf-i[data-sf-module="routes/talk/[id]/page.island0.tsx#default"]';

test("a click to another talk keeps both layouts and the masthead's state and places the new page", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  const crumbs = document.querySelector(".crumbs");
  const before = document.querySelector(TALK_ISLAND);
  expect(before, "the page's own state is an island beside it").toBeTruthy();
  const count = screen.getByLabelText("my schedule");
  await fireEvent.click(count);
  expect(screen.getByText(/Kept in the session cookie/), "the masthead panel is open").toBeTruthy();

  await fireEvent.click(screen.getByText("Loaders that refuse to waterfall"));
  await settle();

  expect(location.pathname).toEqual("/talk/2");
  expect(document.querySelector(".talk h2")?.textContent).toEqual("Loaders that refuse to waterfall");
  expect(document.querySelector(".crumbs"), "the talk layout's DOM was kept").toBe(crumbs);
  expect(screen.getByLabelText("my schedule"), "and the masthead's").toBe(count);
  expect(screen.getByText(/Kept in the session cookie/), "with its state").toBeTruthy();
  const after = document.querySelector(TALK_ISLAND);
  expect(after && after !== before, "the new page placed an island of its own").toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="src/ui/SaveTalk.tsx#default"][data-sf-mounted]'), "the islands the new page places mount").toBeTruthy();
  expect(document.querySelectorAll(TALK_ISLAND).length, "the old page is gone").toEqual(1);
});

test("state the page holds is its own: it survives a click inside the page and starts afresh on the next talk", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  await fireEvent.click(screen.getByText("Hide"));
  expect(document.querySelector(".alongside li"), "the page's own state hid the list").toBeNull();
  expect(screen.getByText("Show")).toBeTruthy();
  await fireEvent.click(screen.getByText("Show"));
  await fireEvent.click(screen.getByText("Loaders that refuse to waterfall"));
  await settle();
  expect(document.querySelector(".talk h2")?.textContent).toEqual("Loaders that refuse to waterfall");
  expect(screen.getByText("Hide"), "the new talk is a new instance of the page").toBeTruthy();
});

test("a click back to the day replaces the talk layout with the day", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  await fireEvent.click(screen.getByText("The day"));
  await settle();
  expect(location.pathname).toEqual("/");
  expect(document.querySelectorAll(".talk-row").length, "the day rendered under the root layout").toEqual(2);
  expect(document.querySelector(".crumbs"), "the talk layout is gone").toBeNull();
});

const OPENING = 'sf-i[data-sf-module="src/ui/Opening.tsx#default"]';

/** Clicks `link` without settling, so the navigation it starts is still in flight when the caller looks. */
async function startFollowing(link: Element): Promise<void> {
  link.dispatchEvent(createEvent("click", link));
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

test("the link a click follows is pending until the page it names is shown", async () => {
  await load("/", { ctx: day() });
  await settle();
  const link = screen.getByText("Loaders that refuse to waterfall").closest("a");
  expect(link?.querySelector(OPENING), "the link holds an island reading its status").toBeTruthy();
  await startFollowing(link as Element);
  expect(link?.hasAttribute("data-sf-pending"), "the link is marked while the payload is fetched").toBe(true);
  expect(link?.querySelector(".opening-on")?.textContent, "and the island inside it says so").toEqual("opening…");
  await settle();
  expect(location.pathname).toEqual("/talk/2");
  expect(document.querySelector("[data-sf-pending]"), "the mark is gone once the page is shown").toBeNull();
});

test("a Link an island renders tells the component inside it that its navigation is pending", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  const link = screen.getByText("Loaders that refuse to waterfall").closest("a");
  expect(link?.querySelector(".opening"), "the island's own link holds the component").toBeTruthy();
  await startFollowing(link as Element);
  expect(link?.querySelector(".opening-on")?.textContent).toEqual("opening…");
  await settle();
  expect(location.pathname).toEqual("/talk/2");
  expect(document.querySelector(".opening-on"), "nothing is pending once the page is shown").toBeNull();
});

const card = (property: string) => document.head.querySelector(`meta[property="${property}"]`)?.getAttribute("content");
const blocks = () => Array.from(document.head.querySelectorAll('script[type="application/ld+json"]')).map((s) => s.textContent);

test("a navigation writes the head rows of the page it lands on and takes away the last page's", async () => {
  await load("/", { ctx: day() });
  await settle();
  const title = document.title;
  expect(card("og:type"), "the home page names none").toBeUndefined();
  expect(blocks()).toEqual([]);

  await fireEvent.click(screen.getByText("Venue"));
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(card("og:type"), "the venue's own row").toEqual("place");
  expect(blocks(), "its data block, with its text").toEqual(['{"@type":"Place"}']);
  expect(card("og:title"), "the card follows the page's title").toEqual(document.title);

  await fireEvent.click(document.querySelector('a[href="/"]')!);
  await settle();
  expect(location.pathname).toEqual("/");
  expect(card("og:type"), "taken away").toBeUndefined();
  expect(blocks(), "taken away").toEqual([]);
  expect(card("og:title"), "and the home page's card is back").toEqual(title);
  expect(document.head.querySelectorAll('meta[property="og:title"]').length, "one card, however many pages were visited").toEqual(1);
});

test("a row both pages carry unchanged keeps its element", async () => {
  await load("/", { ctx: day() });
  await settle();
  const icon = document.head.querySelector('link[rel="icon"]');
  expect(icon, "the document links an icon").toBeTruthy();
  await fireEvent.click(screen.getByText("Venue"));
  await settle();
  expect(document.head.querySelector('link[rel="icon"]'), "not taken out and put back, which would refetch it").toBe(icon);
});
