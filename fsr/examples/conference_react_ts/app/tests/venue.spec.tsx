import { ctx, expect, fireEvent, load, render, screen, settle, test } from "@snapfire/fsr-client/testing";

import LocalClock from "@src/ui/LocalClock";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "09:30", ends: "10:10", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
  { id: "3", title: "Caching what you meant", track: "Data", room: "Room 2", starts: "10:20", ends: "11:00", speaker: "Cy Mbeki", level: "intro", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

test("the venue page is composition with an island the browser renders alone and a Vue island in it", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  expect(screen.getByText("The Old Exchange"), "the page is the server's markup").toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/venue/page.tsx#default"]'), "no framework holds the page").toBeNull();
  const clock = document.querySelector('sf-i[data-sf-module="src/ui/LocalClock.tsx#default"][data-sf-mounted]');
  expect(clock, "the clock reads the visitor's time zone, which the server cannot, so the browser renders it alone").toBeTruthy();
  expect(clock!.querySelector(".local-clock")?.textContent?.startsWith("Times are the venue's. You are in "), clock!.innerHTML).toBeTruthy();
  expect(screen.getByLabelText("my schedule"), "and the layout around it").toBeTruthy();
  const rooms = document.querySelectorAll(".floors-room");
  expect(rooms.length, "one button per room").toEqual(2);
  await fireEvent.click(screen.getByText("Room 2"));
  await settle();
  expect(document.querySelectorAll(".floors-talks li").length, "the island's own state").toEqual(2);
});

test("one page under one layout holds an island of every owner", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  const owner = (module: string) => document.querySelector(`sf-i[data-sf-module="${module}"]`);
  expect(owner("src/ui/Saved.tsx#default")?.hasAttribute("data-sf-mounted"), "React, in the layout").toEqual(true);
  expect(owner("src/ui/Floors.vue#default")?.hasAttribute("data-sf-mounted"), "Vue").toEqual(true);
  expect(owner("src/ui/LocalClock.tsx#default")?.hasAttribute("data-sf-mounted"), "React, mounted fresh since the server cannot render it").toEqual(true);
  const count = document.querySelector("talk-count");
  expect(count?.hasAttribute("data-upgraded"), "the custom element defined itself").toEqual(true);
  expect(count?.textContent).toEqual("3 talks today");
  expect(owner("src/ui/AskDesk.tsx#default")?.closest("sf-s")?.getAttribute("data-sf-mode"), "the help desk steps on the server").toEqual("server");
  await fireEvent.click(screen.getByText("Ask the help desk"));
  await settle();
  expect(screen.getByText("Asked 1 time"), "the server answered the click").toBeTruthy();
});

test("the clock renders on its own through React, with nothing from the server to hydrate", async () => {
  const r = await render(<LocalClock />);
  expect(r.hydrated, "the server could not render it").toBeNull();
  expect(r.container.querySelector("sf-i")?.hasAttribute("data-sf-mounted"), "React's mounter ran").toEqual(true);
  expect(r.getByText(/You are in/).textContent).toMatch(/Times are the venue's\. You are in .+\./);
});
