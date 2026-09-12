import { drop, save } from "@routes/talk/[id]/actions";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const one = { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" };
const two = { ...one, id: "2", title: "Loaders that refuse to waterfall" };

test("saving keeps the id and reports how many are held", async () => {
  const c = ctx<{ talk_id: string }>({ session: { saved: {} }, input: { talk_id: "2" }, services: { program: { listTalks: () => [one, two] } } });
  const result = await save(c);
  assert.equal(c.session.saved, { "2": true });
  assert.equal(result.saved, 1);
  assert.equal(c.trace.session.written, ["saved"]);
});

test("saving a talk twice is the same one talk", async () => {
  const c = ctx<{ talk_id: string }>({ session: { saved: { "2": true } }, input: { talk_id: "2" }, services: { program: { listTalks: () => [one, two] } } });
  const result = await save(c);
  assert.equal(result.saved, 1);
});

test("an id off the programme is refused and the session is left alone", async () => {
  const c = ctx<{ talk_id: string }>({ session: { saved: {} }, input: { talk_id: "99" }, services: { program: { listTalks: () => [one, two] } } });
  await assert.rejects(save(c), "not_found");
  assert.equal(c.trace.session.written, []);
});

test("dropping takes one out and leaves the rest", async () => {
  const c = ctx<{ talk_id: string }>({ session: { saved: { "1": true, "2": true } }, input: { talk_id: "1" } });
  const result = await drop(c);
  assert.equal(c.session.saved, { "2": true });
  assert.equal(result.saved, 1);
});

test("dropping one that is not held is refused before anything is written", async () => {
  const c = ctx<{ talk_id: string }>({ session: { saved: { "2": true } }, input: { talk_id: "1" } });
  await assert.rejects(drop(c), "not_found");
  assert.equal(c.trace.session.written, []);
});
