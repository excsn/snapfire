import { load } from "@routes/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden", "Workshop"] };
const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya", deposit: 15, days: 7 };

test("the shelves hold every tool when no category is asked for", async () => {
  const c = ctx({ services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { category, categories, tools } = await load(c);
  assert.equal(category, "all");
  assert.equal(categories, ["Garden", "Workshop"]);
  assert.equal(tools.length, 2);
});

test("a category in the query keeps only that shelf", async () => {
  const c = ctx({ query: { category: "Workshop" }, services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { tools } = await load(c);
  assert.equal(tools, [drill]);
});

test("the two calls leave together", async () => {
  const c = ctx({ services: { shed: { listTools: () => [trimmer], getShed: () => shed } } });
  await load(c);
  assert.equal(c.trace.calls.length, 2);
});

test("what is reserved comes out of the session as ids", async () => {
  const c = ctx({ session: { reserved: { "3": 5 } }, services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { reserved } = await load(c);
  assert.equal(reserved, ["3"]);
});
