import { ctx, expect, fireEvent, load, settle, test, waitFor } from "@snapfire/fsr-client/testing";
import { get, set } from "@snapfire/fsr-client/store";

import { reservedCount } from "@src/store";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya" };
const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden", "Workshop"] };

const stocked = () =>
  ctx({
    session: { reserved: { "3": true } },
    services: { shed: { getShed: () => shed, listTools: () => [trimmer, drill], listLoans: () => [], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });

const names = () => [...document.querySelectorAll(".tool-title")].map((a) => a.textContent);

test("a shelf chip swaps the page in through htmx and the settle hands its seed to the store", async () => {
  await load("/", { ctx: stocked() });
  await settle();
  expect(names()).toEqual(["Hedge trimmer", "Cordless drill"]);
  set(reservedCount, -1);
  const chip = [...document.querySelectorAll(".chip")].find((a) => a.textContent?.trim() === "Workshop")!;
  await fireEvent.click(chip);
  await waitFor(() => expect(names()).toEqual(["Cordless drill"]));
  expect(document.querySelector(".chip-on")?.textContent?.trim(), "the swapped page marks its own chip").toEqual("Workshop");
  await waitFor(() => expect(get(reservedCount), "the fragment's seed, read once htmx settled").toEqual(1));
});
