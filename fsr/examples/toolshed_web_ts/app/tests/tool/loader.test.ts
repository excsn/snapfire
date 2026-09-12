import { load } from "@routes/tool/[id]/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const washer = { ...trimmer, id: "2", name: "Pressure washer", keeper: "Ama", deposit: 30, days: 2 };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya", deposit: 15, days: 7 };

test("a tool is found by its id with the rest of its shelf beside it", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "1" }, services: { shed: { listTools: () => [trimmer, washer, drill] } } });
  const { tool, sameCategory, reserved } = await load(c);
  assert.equal(tool.name, "Hedge trimmer");
  assert.equal(sameCategory, [washer]);
  assert.equal(reserved, false);
});

test("a reservation in the session shows on the tool", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "1" }, session: { reserved: { "1": true } }, services: { shed: { listTools: () => [trimmer] } } });
  const { reserved } = await load(c);
  assert.equal(reserved, true);
});

test("an id the shed does not hold is not found", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "9" }, services: { shed: { listTools: () => [trimmer] } } });
  await assert.rejects(load(c), "not_found");
});
