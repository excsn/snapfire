import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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

const TALK_LAYOUT = 'sf-i[data-sf-module="routes/talk/layout.tsx#default"]';
const TALK_PAGE = 'sf-i[data-sf-module="routes/talk/[id]/page.tsx#default"]';

test("the talk layout is one React tree with the talk inside it", async () => {
  await load("/talk/1", { ctx: day() });
  const layout = document.querySelector(`${TALK_LAYOUT}[data-sf-mounted]`);
  expect(layout, "the talk layout hydrated").toBeTruthy();
  const page = layout!.querySelector(TALK_PAGE)!;
  expect(page.hasAttribute("data-sf-mounted"), "the page is mounted by the layout's root, not by a scan").toBeTruthy();
  expect(document.querySelector(`script[data-sf-props="${page.id}"]`), "its props script was consumed by the hydration").toBeNull();
  expect(page.parentElement?.tagName, "the page sits in the layout's child region").toEqual("SF-S");
  expect(page.previousSibling?.nodeType, "under the region's delimiters").toEqual(8);
  await settle();
  expect(page.querySelector('sf-i[data-sf-module="src/ui/SaveTalk.tsx#default"][data-sf-mounted]'), "an island placed by the page mounts in a root of its own").toBeTruthy();
  const root = document.querySelector('sf-i[data-sf-module="routes/layout.tsx#default"][data-sf-mounted]');
  expect(root, "the root layout is a tree too").toBeTruthy();
  const below = root!.querySelector(TALK_LAYOUT)!;
  expect(below.hasAttribute("data-sf-scheduled"), "a layout below a tree root is a root of its own, mounted by the scan").toBeTruthy();
  expect(document.querySelector(`script[data-sf-props="${below.id}"]`), "with its props script left for that mount").toBeTruthy();
  expect(below.parentElement?.parentElement?.closest("sf-i"), "in the root's adopted child region").toBe(root);
});

test("a click to another talk renders the new page from its props inside the live layout", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  const crumbs = document.querySelector(".crumbs");
  const before = document.querySelector(TALK_PAGE)!;
  const count = screen.getByLabelText("my schedule");
  await fireEvent.click(count);
  expect(screen.getByText(/Kept in the session cookie/), "the masthead panel is open").toBeTruthy();

  await fireEvent.click(screen.getByText("Loaders that refuse to waterfall"));
  await settle();

  expect(location.pathname).toEqual("/talk/2");
  expect(document.querySelector(".talk h2")?.textContent).toEqual("Loaders that refuse to waterfall");
  const after = document.querySelector(TALK_PAGE)!;
  expect(after, "the page marker is rendered by React").toBeTruthy();
  expect(after === before, "a new instance of the page").toEqual(false);
  expect(after.id.startsWith("sf-t"), `the marker carries an id the adapter gave it: ${after.id}`).toBeTruthy();
  expect(after.hasAttribute("data-sf-mounted"), "and is mounted by the layout's root").toBeTruthy();
  expect(document.querySelector(".crumbs"), "the talk layout's DOM was kept").toBe(crumbs);
  expect(screen.getByLabelText("my schedule"), "and the masthead's").toBe(count);
  expect(screen.getByText(/Kept in the session cookie/), "with its state").toBeTruthy();
  expect(after.querySelector('sf-i[data-sf-module="src/ui/SaveTalk.tsx#default"][data-sf-mounted]'), "the islands the new page places mount").toBeTruthy();
  expect(after.previousSibling instanceof Comment && after.previousSibling.data.startsWith("sf-g:"), "the region is delimited for the next navigation").toBeTruthy();
  expect(document.querySelectorAll(TALK_PAGE).length, "the old page is gone").toEqual(1);
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

test("a click back to the day leaves the tree for a page the root adopts", async () => {
  await load("/talk/1", { ctx: day() });
  await settle();
  await fireEvent.click(screen.getByText("The day"));
  await settle();
  expect(location.pathname).toEqual("/");
  expect(document.querySelectorAll(".talk-row").length, "the day rendered under the root layout").toEqual(2);
  expect(document.querySelector(TALK_LAYOUT), "the talk layout is gone with its tree").toBeNull();
});
