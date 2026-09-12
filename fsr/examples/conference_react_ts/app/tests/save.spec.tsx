import { assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime"] };

test("adding a talk writes the session and the count in the masthead moves with it", async () => {
  const c = ctx({
    session: { saved: {} },
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/talk/1", { ctx: c });
  assert.equal(screen.getByLabelText("my schedule").textContent, "0 saved");

  await fireEvent.click(screen.getByText("Add to my schedule"));
  await settle();

  assert.equal(c.session.saved, { "1": true }, "the action wrote the session through the interpreter");
  assert.equal(screen.getByLabelText("my schedule").textContent, "1 saved", "and the header island followed the store");
  assert.ok(screen.getByText("On your schedule"));
});

test("dropping one already kept takes it back out and the count follows", async () => {
  const c = ctx({
    session: { saved: { "1": true } },
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/talk/1", { ctx: c });
  assert.ok(screen.getByText("On your schedule"), "the loader read the session");

  await fireEvent.click(screen.getByText("On your schedule"));
  await settle();

  assert.equal(c.session.saved, {}, "dropping it took it back out");
  assert.equal(screen.getByLabelText("my schedule").textContent, "0 saved");
});
