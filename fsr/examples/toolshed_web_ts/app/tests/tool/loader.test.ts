import { load, meta } from "@routes/tool/[id]/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const washer = { ...trimmer, id: "2", name: "Pressure washer", keeper: "Ama", deposit: 30, days: 2 };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya", deposit: 15, days: 7 };

test("a tool is found by its id with the rest of its shelf beside it", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "1" }, services: { shed: { listTools: () => [trimmer, washer, drill] } } });
  const { tool, sameCategory, reserved, days } = await load(c);
  expect(tool.name).toEqual("Hedge trimmer");
  expect(sameCategory).toEqual([washer]);
  expect(reserved).toEqual(false);
  expect(days, "the slider starts at what the shed lends it for").toEqual(3);
});

test("a reservation in the session shows on the tool", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "1" }, session: { reserved: { "1": 2 } }, services: { shed: { listTools: () => [trimmer] } } });
  const { reserved, days } = await load(c);
  expect(reserved).toEqual(true);
  expect(days, "the length that was agreed, not the shed's limit").toEqual(2);
});

test("an id the shed does not hold is not found", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "9" }, services: { shed: { listTools: () => [trimmer] } } });
  await expect(load(c)).rejects.toMatchObject({ kind: "not_found" });
});

test("the document is titled from the tool the loader found", async () => {
  const c = ctx<void, "/tool/{id}">({ params: { id: "3" }, services: { shed: { listTools: () => [trimmer, drill] } } });
  const data = await load(c);
  const head = meta({ data });
  expect(head.title).toEqual("Cordless drill · The Shed");
  expect(head.description).toEqual("Priya's, 7 days at a time");
});
