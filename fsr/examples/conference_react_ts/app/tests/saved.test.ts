import { load } from "@routes/saved/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const one = { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" };
const two = { ...one, id: "2", title: "Loaders that refuse to waterfall" };

test("my schedule is the day filtered by the session and nothing else", async () => {
  const c = ctx({ session: { saved: { "2": true } }, services: { program: { listTalks: () => [one, two] } } });
  const { mine } = await load(c);
  assert.equal(mine, [two]);
});

test("an empty session keeps the programme call and returns nothing", async () => {
  const c = ctx({ services: { program: { listTalks: () => [one, two] } } });
  const { mine } = await load(c);
  assert.equal(mine, []);
  assert.equal(c.trace.calls.length, 1);
});
