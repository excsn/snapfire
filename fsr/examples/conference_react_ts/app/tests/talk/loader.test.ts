import { load } from "@routes/talk/[id]/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const one = { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" };
const two = { ...one, id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2" };
const three = { ...one, id: "3", title: "The backend is down", track: "Practice", room: "Room 3", starts: "11:10" };

test("the talk is picked out of the list and what clashes with it comes with it", async () => {
  const c = ctx<void, "/talk/{id}">({ params: { id: "1" }, services: { program: { listTalks: () => [one, two, three] } } });
  const { talk, alongside, saved } = await load(c);
  assert.equal(talk.title, "A plan is not a program");
  assert.equal(alongside, [two], "the same slot, a different room");
  assert.equal(saved, false);
});

test("an id nothing on the programme carries is refused as not_found", async () => {
  const c = ctx<void, "/talk/{id}">({ params: { id: "99" }, services: { program: { listTalks: () => [one, two, three] } } });
  await assert.rejects(load(c), "not_found");
});

test("a talk already kept says so, from the session alone", async () => {
  const c = ctx<void, "/talk/{id}">({ params: { id: "2" }, session: { saved: { "2": true } }, services: { program: { listTalks: () => [one, two, three] } } });
  const { saved } = await load(c);
  assert.equal(saved, true);
});
