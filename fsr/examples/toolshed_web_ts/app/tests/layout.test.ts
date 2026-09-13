import { load, store } from "@routes/layout.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden", "Workshop"] };

test("the layout seeds the tally key with the count it loaded", async () => {
  const c = ctx({ session: { reserved: { "1": 2, "7": 3 } }, services: { shed: { getShed: () => shed } } });
  const data = await load(c);
  assert.equal(data.reserved, 2);
  const seeded = store({ data });
  assert.equal(seeded["shed/reserved"], 2);
});

test("an empty session seeds a zero rather than nothing", async () => {
  const c = ctx({ session: { reserved: {} }, services: { shed: { getShed: () => shed } } });
  const data = await load(c);
  const seeded = store({ data });
  assert.equal(seeded["shed/reserved"], 0);
});
