import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "Two batteries." };
const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden"] };

const stocked = (session = {}) =>
  ctx({
    session,
    services: { shed: { getShed: () => shed, listTools: () => [trimmer], listLoans: () => [{ tool: "Gazebo", who: "the Okafors", back: "2026-09-14" }], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });

test("the masthead is kept across a navigation and the page beneath it is replaced", async () => {
  await load("/", { ctx: stocked() });
  const tally = document.querySelector("shed-tally");
  expect(tally, "the custom element is in the server's markup").toBeTruthy();

  await fireEvent.click(screen.getByText("Hedge trimmer"));

  expect(location.pathname).toEqual("/tool/1");
  expect(document.querySelector(".tool h2"), "the page region was replaced").toBeTruthy();
  expect(document.querySelector("shed-tally"), "the layout's DOM, the element included, was kept").toBe(tally);
});

test("the tally is seeded by the layout loader and written into the store", async () => {
  await load("/", { ctx: stocked({ reserved: { "1": 3 } }) });
  expect(screen.getByLabelText("reserved").textContent?.trim()).toEqual("1 reserved");
  const seed = document.querySelector("script[data-sf-store]");
  expect(seed?.textContent?.includes("shed/reserved"), "the store seed names the key the element follows").toBeTruthy();
});

test("the one island is an element definition; no framework mounts", async () => {
  await load("/tool/1", { ctx: stocked() });
  const markers = Array.from(document.querySelectorAll("sf-i"));
  expect(markers.length, "the loans list, whose definition is imported when it scrolls into view").toEqual(1);
  expect(markers[0].getAttribute("data-sf-module")).toEqual("src/elements/time-ago.ts#default");
  expect(markers[0].hasAttribute("data-sf-mounted"), "the harness has no layout, so its observer reports the panel in view at once and the definition is imported").toBeTruthy();
  expect(markers[0].innerHTML.includes("<time-ago"), "the element markup the server wrote sits inside the marker, waiting for its definition").toBeTruthy();
  expect(document.querySelector('script[data-sf-props="sf-i0"]')?.textContent, "its props are the region key and nothing else").toEqual('{"$k":"routes/slots/loans/page.tsx#default|i0"}');
  expect(await (await fetch("/tool/1")).text(), "the planner's shadow root is written by the server").toMatch(/<loan-planner[^>]*>\s*<template shadowrootmode="open"/);
  expect(document.querySelector("loan-planner")?.shadowRoot?.querySelector("input[name=days]"), "and the element takes it as its shadow root").toBeTruthy();
  expect(document.querySelector("form.reserve input[name=_csrf]"), "the form carries the token").toBeTruthy();
});

test("an element on a later page is upgraded by the definition an earlier page's modules made", async () => {
  await load("/", { ctx: stocked() });
  await load("/", { ctx: stocked() });
  const Tally = customElements.get("shed-tally");
  expect(Tally, "defined once, by the entry module the first page imported").toBeDefined();
  expect(document.querySelector("shed-tally") instanceof Tally!).toBe(true);
});
