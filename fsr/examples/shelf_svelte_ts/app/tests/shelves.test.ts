import { load } from "@routes/page.loader";
import { load as layout } from "@routes/layout.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("every book is on the shelf with no filter, placed by the session", async () => {
  const c = ctx<void, "/">({ session: { reading: "piranesi", progress: { piranesi: 12, stoner: 278 }, done: { stoner: true } } });
  const { shelf, rows } = await load(c);
  expect(shelf).toEqual("all");
  expect(rows.map((row) => row.id)).toEqual(["earthsea", "piranesi", "remains", "stoner", "dispossessed", "gilead"]);
  expect(rows.map((row) => row.place)).toEqual(["to-read", "reading", "to-read", "done", "to-read", "to-read"]);
});

test("each shelf holds the books the session puts on it", async () => {
  const reading = ctx<void, "/">({ query: { shelf: "reading" }, session: { reading: "piranesi", progress: { piranesi: 12, stoner: 278 }, done: { stoner: true } } });
  const onReading = await load(reading);
  expect(onReading.rows.map((row) => row.id)).toEqual(["piranesi"]);
  const done = ctx<void, "/">({ query: { shelf: "done" }, session: { reading: "piranesi", progress: { piranesi: 12, stoner: 278 }, done: { stoner: true } } });
  const onDone = await load(done);
  expect(onDone.rows.map((row) => row.id)).toEqual(["stoner"]);
});

test("the layout counts the finished books and names the one being read", async () => {
  const held = ctx({ session: { reading: "piranesi", progress: { piranesi: 12, stoner: 278 }, done: { stoner: true } } });
  expect(await layout(held)).toEqual({ done: 1, total: 6, reading: "Piranesi" });
  const fresh = ctx({});
  expect(await layout(fresh)).toEqual({ done: 0, total: 6, reading: "" });
});
