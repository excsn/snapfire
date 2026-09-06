import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const board = {
  at: "08:25",
  arrivals: [
    { flight: "LH 906", city: "Frankfurt", scheduled: "07:45", expected: "07:45", status: "landed", code: "landed", gate: "B04" },
    { flight: "AF 1680", city: "Paris CDG", scheduled: "08:05", expected: "08:40", status: "delayed", code: "delayed", gate: "B14" },
  ],
  departures: [{ flight: "AZ 205", city: "Rome", scheduled: "08:20", expected: "08:45", status: "boarding", code: "boarding", gate: "B02" }],
};

const field = () =>
  ctx({
    services: {
      board: {
        getBoard: () => board,
        getWeather: () => ({ field: "clear", wind: "090° at 4 kt", visibility: "10 km", celsius: 17 }),
        listGateChanges: () => [{ flight: "AF 1680", was: "B11", now: "B14", at: "07:32" }],
      },
    },
  });

test("both boards render a row per flight with its status as a class", async () => {
  await load("/", { ctx: field() });
  assert.equal(document.querySelector(".clock strong")?.textContent, "08:25");
  const tables = Array.from(document.querySelectorAll(".table h2")).map((h) => h.textContent);
  assert.equal(tables.join(", "), "Arrivals, Departures", "one table each way");
  const rows = Array.from(document.querySelectorAll("tbody tr"));
  assert.equal(rows.length, 3, "two arrivals and one departure");
  assert.equal(rows[1]?.className, "status-delayed", "the class comes from the row's own code, never from parsing its words");
  assert.equal(rows[2]?.querySelector(".gate")?.textContent, "B02");
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

test("the page carries the island that follows the field", async () => {
  await load("/", { ctx: field() });
  const live = document.querySelector(".live");
  assert.ok(live, "the live pill is in the layout");
  assert.equal(live?.textContent, "live");
});
