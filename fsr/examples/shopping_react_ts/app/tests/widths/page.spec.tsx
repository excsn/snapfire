import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

test("an int64 past 2^53 renders exactly beside its lossy JSON reading", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [], getLedger: () => ({ sequence: 9007199254740993n, issued: 9223372036854775807n, note: "both past 2^53" }) } } });
  await load("/widths", { ctx: c });
  assert.ok(screen.getByText("9007199254740993"), "the exact digits");
  assert.ok(screen.getByText("9007199254740992"), "the JSON reading, one off");
  assert.ok(screen.getByText("9223372036854775807"), "i64::MAX exact");
  assert.ok(screen.getByText("9223372036854776000"), "i64::MAX as a double");
});
