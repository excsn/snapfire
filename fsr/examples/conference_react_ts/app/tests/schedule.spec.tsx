import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

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
  assert.equal(document.querySelector(".masthead h1")?.textContent, "Everything At Once");
  const rows = Array.from(document.querySelectorAll(".talk-row"));
  assert.equal(rows.length, 2, "one row per talk");
  assert.equal(rows[0]?.querySelector(".talk-title")?.textContent, "A plan is not a program");
  assert.equal(rows[1]?.querySelector(".room")?.textContent, "Room 2");
});

test("one slot answers while the other is down and the page is whole either way", async () => {
  await load("/", { ctx: day() });
  assert.ok(screen.getByText("Registration has moved to the west door."), "the announcements slot filled");
  assert.ok(document.querySelector(".sponsors.panel-down"), "the sponsors slot fell back to its own error boundary");
  assert.equal(document.querySelectorAll(".talk-row").length, 2, "and the page beside it is untouched");
  assert.equal(document.querySelectorAll(".skeleton").length, 0, "both slots settled, so no fallback is left");
});

test("the track chips carry the query the loader reads back", async () => {
  const filtered = ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/?track=Data", { ctx: filtered });
  assert.equal(document.querySelectorAll(".talk-row").length, 1);
  assert.equal(document.querySelector(".chip-on")?.textContent, "Data");
});
