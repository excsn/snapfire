import { release, reserve } from "@routes/tool/[id]/actions";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
test("reserving a tool the shed holds writes the agreed length and answers the count", async () => {
  const c = ctx<{ tool_id: string; days?: number }>({ input: { tool_id: "1", days: 2 }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const r = await reserve(c);
  assert.equal(r.reserved, 1);
  assert.equal(r.days, 2);
  assert.equal(c.session.reserved, { "1": 2 });
});

test("asking for longer than the shed lends it takes the tool's own limit", async () => {
  const c = ctx<{ tool_id: string; days?: number }>({ input: { tool_id: "1", days: 10 }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const r = await reserve(c);
  assert.equal(r.days, 3);
  assert.equal(c.session.reserved, { "1": 3 });
});

test("a length under a day is a day", async () => {
  const c = ctx<{ tool_id: string; days?: number }>({ input: { tool_id: "1", days: 0 }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const r = await reserve(c);
  assert.equal(r.days, 1);
});

test("a form that posted no length at all reserves it for the tool's own", async () => {
  const c = ctx<{ tool_id: string; days?: number }>({ input: { tool_id: "1" }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const r = await reserve(c);
  assert.equal(r.days, 3);
});

test("reserving one the shed does not hold is refused and the session is untouched", async () => {
  const c = ctx<{ tool_id: string; days?: number }>({ input: { tool_id: "9", days: 2 }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  await assert.rejects(reserve(c), "not_found");
  assert.equal(c.session.reserved, {});
});

test("releasing takes it back out and leaves the others with their lengths", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "1" }, session: { reserved: { "1": 2, "7": 3 } } });
  const r = await release(c);
  assert.equal(r.reserved, 1);
  assert.equal(c.session.reserved, { "7": 3 });
});

test("releasing one that was never reserved is refused", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "1" }, session: { reserved: {} } });
  await assert.rejects(release(c), "not_found");
});
