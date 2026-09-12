import { release, reserve } from "@routes/tool/[id]/actions";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };

test("reserving a tool the shed holds writes the session and answers the count", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "1" }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const r = await reserve(c);
  assert.equal(r.reserved, 1);
  assert.equal(c.session.reserved, { "1": true });
});

test("reserving one the shed does not hold is refused and the session is untouched", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "9" }, session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  await assert.rejects(reserve(c), "not_found");
  assert.equal(c.session.reserved, {});
});

test("releasing takes it back out and leaves the others", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "1" }, session: { reserved: { "1": true, "7": true } } });
  const r = await release(c);
  assert.equal(r.reserved, 1);
  assert.equal(c.session.reserved, { "7": true });
});

test("releasing one that was never reserved is refused", async () => {
  const c = ctx<{ tool_id: string }>({ input: { tool_id: "1" }, session: { reserved: {} } });
  await assert.rejects(release(c), "not_found");
});
