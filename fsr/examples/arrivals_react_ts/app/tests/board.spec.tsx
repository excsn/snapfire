import { ctx, expect, load, test } from "@snapfire/fsr-client/testing";

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
  expect(document.querySelector(".clock strong")?.textContent).toEqual("08:25");
  const tables = Array.from(document.querySelectorAll(".table h2")).map((h) => h.textContent);
  expect(tables.join(", "), "one table each way").toEqual("Arrivals, Departures");
  const rows = Array.from(document.querySelectorAll("tbody tr"));
  expect(rows.length, "two arrivals and one departure").toEqual(3);
  expect(rows[1]?.className, "the class comes from the row's own code, never from parsing its words").toEqual("status-delayed");
  expect(rows[2]?.querySelector(".gate")?.textContent).toEqual("B02");
});

test("each panel is a slot of its own, filled from its own service", async () => {
  await load("/", { ctx: field() });
  const panels = Array.from(document.querySelectorAll(".panel h2")).map((h) => h.textContent);
  expect(panels.includes("The field"), `the weather panel is placed, got ${panels.join(", ")}`).toBeTruthy();
  expect(panels.includes("Gate changes"), `the gate panel is placed, got ${panels.join(", ")}`).toBeTruthy();
  expect(document.querySelector(".weather .reading")?.textContent).toEqual("clear");
  expect(document.querySelector(".gates .now")?.textContent).toEqual("B14");
  expect(document.querySelectorAll(".skeleton").length, "every panel answered, so no skeleton is left").toEqual(0);
});

test("the page carries the island that follows the field", async () => {
  await load("/", { ctx: field() });
  const live = document.querySelector(".live");
  expect(live, "the live pill is in the layout").toBeTruthy();
  expect(live?.textContent).toEqual("live");
});
