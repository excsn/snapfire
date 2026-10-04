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
