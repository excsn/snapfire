import { ctx, expect, fireEvent, load, settle, test } from "@snapfire/fsr/client/testing";

const titles = () => [...document.querySelectorAll(".ranks tbody th")].map((th) => th.textContent);

test("the stats page lists every book with its share and the total the session holds", async () => {
  const c = ctx({ session: { reading: "", progress: { stoner: 139, gilead: 247 }, done: { gilead: true } } });
  await load("/stats", { ctx: c });
  expect(titles()).toEqual(["A Wizard of Earthsea", "Gilead", "Piranesi", "Stoner", "The Dispossessed", "The Remains of the Day"]);
  expect(document.querySelector(".note strong")?.textContent).toEqual("386");
  expect(document.querySelector(".ranks caption")?.textContent).toEqual("6 books");
  expect(document.querySelector("tr.done th")?.textContent).toEqual("Gilead");
  expect(document.querySelector(".legend h3")?.textContent).toEqual("About 64 pages a book");
});

test("ordering by length and switching to dots redraw the table in the browser", async () => {
  const c = ctx({ session: { reading: "", progress: { stoner: 139 }, done: {} } });
  await load("/stats", { ctx: c });
  const select = document.querySelector(".controls select") as HTMLSelectElement;
  select.value = "pages";
  await fireEvent.change(select);
  await settle();
  expect(titles()[0]).toEqual("The Dispossessed");
  await fireEvent.click(document.querySelector(".controls input") as HTMLInputElement);
  await settle();
  expect(document.querySelectorAll(".ranks .dots").length).toEqual(6);
  expect(document.querySelectorAll(".ranks .bar").length).toEqual(0);
});

test("figures numbered by module state count across the page's islands on the server", async () => {
  const html = await (await fetch("/stats")).text();
  const served = [...html.matchAll(/<p class="figure">([^<]*)<\/p>/g)].map((m) => m[1]);
  expect(served, "one module counter for the whole page, as the browser has").toEqual(["Figure 1: pages read per book", "Figure 2: books finished per month"]);
});

test("a later load starts the page's modules fresh, as a browser's page load does", async () => {
  await load("/stats");
  await load("/stats");
  await settle();
  const hydrated = [...document.querySelectorAll(".figure")].map((p) => p.textContent);
  expect(hydrated, "the module counter starts again with the document").toEqual(["Figure 1: pages read per book", "Figure 2: books finished per month"]);
});
