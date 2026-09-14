import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "10:20", ends: "11:00", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: {
        getConference: () => conference,
        listTalks: () => talks,
        listAnnouncements: () => [{ at: "08:55", text: "Registration has moved to the west door." }],
        listSponsors: () => {
          throw new Error("the sponsors service is not answering");
        },
      },
    },
  });

test("the day is a row per talk under the masthead the layout loaded", async () => {
  await load("/", { ctx: day() });
  expect(document.querySelector(".masthead h1")?.textContent).toEqual("Everything At Once");
  const rows = Array.from(document.querySelectorAll(".talk-row"));
  expect(rows.length, "one row per talk").toEqual(2);
  expect(rows[0]?.querySelector(".talk-title")?.textContent).toEqual("A plan is not a program");
  expect(rows[1]?.querySelector(".room")?.textContent).toEqual("Room 2");
});

test("one slot answers while the other is down and the page is whole either way", async () => {
  await load("/", { ctx: day() });
  expect(screen.getByText("Registration has moved to the west door."), "the announcements slot filled").toBeTruthy();
  expect(document.querySelector(".sponsors.panel-down"), "the sponsors slot fell back to its own error boundary").toBeTruthy();
  expect(document.querySelectorAll(".talk-row").length, "and the page beside it is untouched").toEqual(2);
  expect(document.querySelectorAll(".skeleton").length, "both slots settled, so no fallback is left").toEqual(0);
});

test("the track chips carry the query the loader reads back", async () => {
  const filtered = ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/?track=Data", { ctx: filtered });
  expect(document.querySelectorAll(".talk-row").length).toEqual(1);
  expect(document.querySelector(".chip-on")?.textContent).toEqual("Data");
});
