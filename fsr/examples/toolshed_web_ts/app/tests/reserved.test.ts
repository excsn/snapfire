import { load } from "@routes/reserved/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const gazebo = { ...trimmer, id: "7", name: "Gazebo", category: "Party", keeper: "Tomas", deposit: 40 };

test("the reserved page is the session's tools with their deposits summed", async () => {
  const c = ctx({ session: { reserved: { "1": 2, "7": 1 } }, services: { shed: { listTools: () => [trimmer, gazebo] } } });
  const { reserved, deposit } = await load(c);
  expect(reserved.length).toEqual(2);
  expect(deposit).toEqual(60);
  expect(reserved.map((t) => t.days), "each row carries the length it was reserved for").toEqual([2, 1]);
});

test("an empty session is an empty page", async () => {
  const c = ctx({ session: { reserved: {} }, services: { shed: { listTools: () => [trimmer] } } });
  const { reserved, deposit } = await load(c);
  expect(reserved).toEqual([]);
  expect(deposit).toEqual(0);
});
