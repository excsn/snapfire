import { load } from "@routes/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden", "Workshop"] };
const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya", deposit: 15, days: 7 };

test("the shelves hold every tool when no category is asked for", async () => {
  const c = ctx({ services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { category, categories, tools } = await load(c);
  expect(category).toEqual("all");
  expect(categories).toEqual(["Garden", "Workshop"]);
  expect(tools.length).toEqual(2);
});

test("a category in the query keeps only that shelf", async () => {
  const c = ctx({ query: { category: "Workshop" }, services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { tools } = await load(c);
  expect(tools).toEqual([drill]);
});

test("the two calls leave together", async () => {
  const c = ctx({ services: { shed: { listTools: () => [trimmer], getShed: () => shed } } });
  await load(c);
  expect(c.trace.calls.length).toEqual(2);
});

test("what is reserved comes out of the session as ids", async () => {
  const c = ctx({ session: { reserved: { "3": 5 } }, services: { shed: { listTools: () => [trimmer, drill], getShed: () => shed } } });
  const { reserved } = await load(c);
  expect(reserved).toEqual(["3"]);
});
