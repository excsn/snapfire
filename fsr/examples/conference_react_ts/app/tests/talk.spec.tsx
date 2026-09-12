import { assert, ctx, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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

test("the talk renders under both layouts, with what clashes with it", async () => {
  await load("/talk/1", { ctx: day() });
  assert.ok(document.querySelector(".masthead h1"), "the root layout is above it");
  assert.ok(document.querySelector(".crumbs"), "and the talk layout between them");
  assert.equal(document.querySelector(".talk h2")?.textContent, "A plan is not a program");
  assert.equal(document.querySelector(".alongside li a")?.textContent, "Loaders that refuse to waterfall");
});

test("the save button is an island timed on load and the pace island waits to be seen", async () => {
  await load("/talk/1", { ctx: day() });
  const eager = document.querySelector('sf-s[data-sf-island][data-sf-when="load"] .save');
  assert.ok(eager, "the save control is in a region timed on load");

  const lazy = document.querySelector('sf-s[data-sf-island][data-sf-when="visible"]');
  assert.ok(lazy, "the pace control is in a region timed on being scrolled to");
  assert.ok(lazy!.querySelector(".feedback"), "rendered on the server all the same");
  await settle();
  assert.ok(lazy!.querySelector('sf-i[data-sf-module="src/ui/Feedback.tsx#Feedback"][data-sf-mounted]'), "mounted once the observer reported it in view");
});

test("a talk the programme does not carry degrades to the route's own boundary", async () => {
  await load("/talk/99", { ctx: day() });
  assert.ok(screen.getByText("That talk is not on the programme"));
  assert.ok(document.querySelector(".masthead h1"), "the masthead above it is still there");
});
