import { load } from "@routes/trail/[id]/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("a trail comes with its notes", async () => {
  const c = ctx<void, "/trail/{id}">({ params: { id: "marsh" } });
  const { trail } = await load(c);
  expect(trail.name).toEqual("Saltmarsh Loop");
  expect(trail.notes.length).toEqual(2);
});

test("an id no trail has is refused as not_found", async () => {
  const c = ctx<void, "/trail/{id}">({ params: { id: "moor" } });
  await expect(load(c)).rejects.toMatchObject({ kind: "not_found" });
});
