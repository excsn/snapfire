import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

test("the panel opened in the masthead is still open after the page beneath it changes", async () => {
  await load("/", { ctx: day() });
  const count = screen.getByLabelText("my schedule");
  await fireEvent.click(count);
  assert.ok(screen.getByText(/Kept in the session cookie/), "the panel is open");

  await fireEvent.click(screen.getByText("A plan is not a program"));

  assert.equal(location.pathname, "/talk/1");
  assert.ok(document.querySelector(".talk h2"), "the page region was replaced");
  assert.ok(screen.getByLabelText("my schedule") === count, "the layout's DOM was kept");
  assert.ok(screen.getByText(/Kept in the session cookie/), "and the state inside it");
});

test("the saved count is seeded by the layout loader and read through the store", async () => {
  const held = ctx({
    session: { saved: { "1": true } },
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/", { ctx: held });
  assert.equal(screen.getByLabelText("my schedule").textContent, "1 saved");
});
