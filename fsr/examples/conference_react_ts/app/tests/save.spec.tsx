import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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
  expect(screen.getByLabelText("my schedule").textContent).toEqual("0 saved");

  await fireEvent.click(screen.getByText("Add to my schedule"));
  await settle();

  expect(c.session.saved, "the action wrote the session through the interpreter").toEqual({ "1": true });
  expect(screen.getByLabelText("my schedule").textContent, "and the header island followed the store").toEqual("1 saved");
  expect(screen.getByText("On your schedule")).toBeTruthy();
});

test("dropping one already kept takes it back out and the count follows", async () => {
  const c = ctx({
    session: { saved: { "1": true } },
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });
  await load("/talk/1", { ctx: c });
  expect(screen.getByText("On your schedule"), "the loader read the session").toBeTruthy();

  await fireEvent.click(screen.getByText("On your schedule"));
  await settle();

  expect(c.session.saved, "dropping it took it back out").toEqual({});
  expect(screen.getByLabelText("my schedule").textContent).toEqual("0 saved");
});
