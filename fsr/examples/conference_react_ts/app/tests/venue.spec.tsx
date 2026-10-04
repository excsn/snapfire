import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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
