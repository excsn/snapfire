import { name } from "@routes/actions";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("naming yourself keeps the name and stays where you are", async () => {
  const c = ctx<{ name: string; next?: string }>({ input: { name: "alice" }, session: {} });
  await name(c);
  expect(c.session.name).toEqual("alice");
  expect(c.trace.redirect).toEqual(null);
});

test("naming yourself on the way into a room sends you back to it", async () => {
  const c = ctx<{ name: string; next?: string }>({ input: { name: "alice", next: "/room/lobby" }, session: {} });
  await name(c);
  expect(c.session.name).toEqual("alice");
  expect(c.trace.redirect).toEqual("/room/lobby");
});

test("a next that leaves the site is refused and the name is not kept", async () => {
  const c = ctx<{ name: string; next?: string }>({ input: { name: "alice", next: "//elsewhere.example/" }, session: {} });
  await expect(name(c)).rejects.toMatchObject({ kind: "invalid" });
  expect(c.session.name).toEqual(undefined);
});
