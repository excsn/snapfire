import { load } from "@routes/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const runtime = { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" };
const data = { ...runtime, id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "10:20", ends: "11:00", speaker: "Bea Lindqvist" };
const conference = { name: "Everything At Once", day: "Thursday", venue: "Hall B", tracks: ["Runtime", "Data"] };

test("the talks and the conference are two reads neither of which waits for the other", async () => {
  const c = ctx({ services: { program: { listTalks: () => [runtime, data], getConference: () => conference } } });
  const { talks, tracks } = await load(c);
  assert.equal(talks.length, 2);
  assert.equal(tracks, ["Runtime", "Data"]);
  assert.equal(c.trace.calls, [
    { service: "program", method: "listTalks", args: {} },
    { service: "program", method: "getConference", args: {} },
  ]);
});

test("a track in the query narrows the list and names itself back", async () => {
  const c = ctx({ query: { track: "Data" }, services: { program: { listTalks: () => [runtime, data], getConference: () => conference } } });
  const { track, talks } = await load(c);
  assert.equal(track, "Data");
  assert.equal(talks, [data]);
});

test("no track in the query keeps the whole day", async () => {
  const c = ctx({ services: { program: { listTalks: () => [runtime, data], getConference: () => conference } } });
  const { track, talks } = await load(c);
  assert.equal(track, "all");
  assert.equal(talks.length, 2);
});

test("the ids already kept come back with the day, from a session nothing seeded", async () => {
  const fresh = ctx({ services: { program: { listTalks: () => [runtime, data], getConference: () => conference } } });
  const day = await load(fresh);
  assert.equal(day.saved, [], "the schema's default stands in for the absent key");

  const held = ctx({ session: { saved: { "2": true } }, services: { program: { listTalks: () => [runtime, data], getConference: () => conference } } });
  const mine = await load(held);
  assert.equal(mine.saved, ["2"]);
});
