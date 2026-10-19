import { load } from "@routes/book/[id]/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("a book comes with what the session says of it", async () => {
  const c = ctx<void, "/book/{id}">({ params: { id: "stoner" }, session: { reading: "stoner", progress: { stoner: 90 }, done: {} } });
  const { book, read, done, current } = await load(c);
  expect(book.title).toEqual("Stoner");
  expect([read, done, current]).toEqual([90, false, true]);
});

test("an id the shelf does not hold is refused as not_found", async () => {
  const c = ctx<void, "/book/{id}">({ params: { id: "dune" } });
  await expect(load(c)).rejects.toMatchObject({ kind: "not_found" });
});
