import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const arrivals = [
  { flight: "BA 118", from: "New York JFK", due: "07:20", status: "landed", code: "landed", gate: "A12" },
  { flight: "AF 1680", from: "Paris CDG", due: "08:05", status: "delayed", code: "delayed", gate: "B11" },
];

const field = () =>
  ctx({
    services: {
      board: {
        listArrivals: () => arrivals,
        getWeather: () => ({ field: "clear", wind: "090° at 4 kt", visibility: "10 km", celsius: 17 }),
        listGateChanges: () => [{ flight: "AF 1680", was: "B11", now: "B14" }],
      },
    },
  });

test("the board renders a row per arrival with its status as a class", async () => {
  await load("/", { ctx: field() });
  const rows = Array.from(document.querySelectorAll("tbody tr"));
  assert.equal(rows.length, 2, "one row per arrival");
  assert.equal(rows[1]?.className, "status-delayed", "the class comes from the row's own code, never from parsing its words");
  assert.equal(rows[0]?.querySelector(".flight")?.textContent, "BA 118");
});

test("each panel is a slot of its own, filled from its own service", async () => {
  await load("/", { ctx: field() });
  const panels = Array.from(document.querySelectorAll(".panel h2")).map((h) => h.textContent);
  assert.ok(panels.includes("The field"), `the weather panel is placed, got ${panels.join(", ")}`);
  assert.ok(panels.includes("Gate changes"), `the gate panel is placed, got ${panels.join(", ")}`);
  assert.equal(document.querySelector(".weather .reading")?.textContent, "clear");
  assert.equal(document.querySelector(".gates .now")?.textContent, "B14");
  assert.equal(document.querySelectorAll(".skeleton").length, 0, "every panel answered, so no skeleton is left");
});
