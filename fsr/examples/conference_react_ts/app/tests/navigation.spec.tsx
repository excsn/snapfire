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

test("a click that stays on the page gives a kept custom element the server's new attributes and leaves what it set itself", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  const count = document.querySelector("talk-count") as HTMLElement & { seen?: number };
  count.seen = 1;
  expect(count.textContent).toEqual("2 talks today");
  expect(count.hasAttribute("track"), "no track is chosen").toEqual(false);

  await fireEvent.click(document.querySelector('.venue-tracks a[href="/venue?track=Data"]') as HTMLElement);
  await settle();
  expect(location.search).toEqual("?track=Data");
  expect(document.querySelector("talk-count"), "the island kept its element").toBe(count);
  expect(count.getAttribute("talks"), "an attribute the server changed").toEqual("1");
  expect(count.getAttribute("track"), "an attribute the server added").toEqual("Data");
  expect(count.textContent, "the element read them").toEqual("One Data talk today");
  expect(count.hasAttribute("data-upgraded"), "an attribute it set on itself").toEqual(true);
  expect(count.seen, "a property it was given").toEqual(1);

  await fireEvent.click(document.querySelector('.venue-tracks a[href="/venue"]') as HTMLElement);
  await settle();
  expect(location.search).toEqual("");
  expect(document.querySelector("talk-count")).toBe(count);
  expect(count.hasAttribute("track"), "an attribute the server took away").toEqual(false);
  expect(count.getAttribute("talks")).toEqual("2");
  expect(count.textContent).toEqual("2 talks today");
  expect(count.hasAttribute("data-upgraded"), "still its own").toEqual(true);

  history.back();
  await settle();
  expect(location.search).toEqual("?track=Data");
  expect(document.querySelector("talk-count")).toBe(count);
  expect(count.getAttribute("track"), "a step back is a soft navigation too").toEqual("Data");
  expect(count.textContent).toEqual("One Data talk today");
});

const timetable = [
  { ...talks[0], id: "1", title: "First", track: "Runtime", room: "Hall B", starts: "09:30" },
  { ...talks[0], id: "2", title: "Second", track: "Data", room: "Room 2", starts: "10:20" },
  { ...talks[0], id: "3", title: "Third", track: "Data", room: "Room 2", starts: "11:00" },
];

const timetabled = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => timetable, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

const pickTrack = async (href: string) => {
  await fireEvent.click(document.querySelector(`.venue-tracks a[href="${href}"]`) as HTMLElement);
  await settle();
};

const rows = () => Array.from(document.querySelectorAll(".venue-times li"), (row) => row.textContent?.trim());

test("an island holding only the server's markup takes all of it on a soft navigation, however deep", async () => {
  await load("/venue", { ctx: timetabled() });
  await settle();
  const first = document.querySelector(".venue-first talk-time") as HTMLElement & { seen?: number };
  first.seen = 1;
  const list = document.querySelector(".venue-times ul");
  expect(first.textContent).toEqual("at 09:30");
  expect(rows()).toEqual(["at 09:30 First", "at 10:20 Second", "at 11:00 Third"]);

  await pickTrack("/venue?track=Data");
  expect(document.querySelector(".venue-first talk-time"), "the nested element is kept").toBe(first);
  expect(first.seen, "with what it was given").toEqual(1);
  expect(first.getAttribute("starts"), "and takes the attribute the server changed").toEqual("10:20");
  expect(first.textContent, "and what it writes itself is not replaced by what the server wrote inside it").toEqual("at 10:20");
  expect(document.querySelector(".venue-times ul"), "the list around the rows is kept").toBe(list);
  expect(rows(), "a row the server dropped goes").toEqual(["at 10:20 Second", "at 11:00 Third"]);

  await pickTrack("/venue");
  expect(first.getAttribute("starts")).toEqual("09:30");
  expect(rows(), "a row the server added comes").toEqual(["at 09:30 First", "at 10:20 Second", "at 11:00 Third"]);

  history.back();
  await settle();
  expect(rows(), "a step back takes it too").toEqual(["at 10:20 Second", "at 11:00 Third"]);
});

test("an island whose timing has not fired mounts with what the last navigation gave it", async () => {
  const shown: (() => void)[] = [];
  const observer = globalThis.IntersectionObserver;
  globalThis.IntersectionObserver = class {
    cb: (entries: unknown[]) => void;
    constructor(cb: (entries: unknown[]) => void) {
      this.cb = cb;
    }
    observe(target: Element) {
      shown.push(() => this.cb([{ isIntersecting: true, target }]));
    }
    disconnect() {}
    unobserve() {}
  } as unknown as typeof IntersectionObserver;
  const rooms = () => Array.from(document.querySelectorAll(".floors-room"), (room) => room.textContent?.trim());
  await load("/venue", { ctx: timetabled() });
  await settle();
  const floors = 'sf-i[data-sf-module="src/ui/Floors.vue#default"]';
  expect(document.querySelector(floors)?.hasAttribute("data-sf-mounted"), "out of view, nothing has mounted").toEqual(false);

  await pickTrack("/venue?track=Data");
  expect(rooms(), "the server's markup follows the navigation").toEqual(["Room 2"]);
  globalThis.IntersectionObserver = observer;
  for (const show of shown.splice(0)) show();
  await settle();
  expect(document.querySelector(floors)?.hasAttribute("data-sf-mounted"), "once shown it mounts").toEqual(true);
  expect(rooms(), "from the navigation's props rather than the first page's").toEqual(["Room 2"]);
  await fireEvent.click(screen.getByText("Room 2"));
  await settle();
  expect(document.querySelectorAll(".floors-talks li").length, "and holds them in its own state").toEqual(2);
});
