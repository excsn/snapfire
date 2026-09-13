import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

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
  assert.ok(tally, "the custom element is in the server's markup");

  await fireEvent.click(screen.getByText("Hedge trimmer"));

  assert.equal(location.pathname, "/tool/1");
  assert.ok(document.querySelector(".tool h2"), "the page region was replaced");
  assert.ok(document.querySelector("shed-tally") === tally, "the layout's DOM, the element included, was kept");
});

test("the tally is seeded by the layout loader and written into the store", async () => {
  await load("/", { ctx: stocked({ reserved: { "1": 3 } }) });
  assert.equal(screen.getByLabelText("reserved").textContent?.trim(), "1 reserved");
  const seed = document.querySelector("script[data-sf-store]");
  assert.ok(seed?.textContent?.includes("shed/reserved"), "the store seed names the key the element follows");
});

test("the one island is an element definition; no framework mounts", async () => {
  await load("/tool/1", { ctx: stocked() });
  const markers = Array.from(document.querySelectorAll("sf-i"));
  assert.equal(markers.length, 1, "the loans list, whose definition is imported when it scrolls into view");
  assert.equal(markers[0].getAttribute("data-sf-module"), "src/elements/time-ago.ts#default");
  assert.ok(markers[0].innerHTML.includes("<time-ago"), "the element markup the server wrote sits inside the marker, waiting for its definition");
  assert.equal(document.querySelector('script[data-sf-props="sf-i0"]')?.textContent, '{"$k":"routes/slots/loans/page.tsx#default|i0"}', "its props are the region key and nothing else");
  assert.ok(document.querySelector("loan-planner template[shadowrootmode=open]"), "the planner's shadow root is written by the server");
  assert.ok(document.querySelector("form.reserve input[name=_csrf]"), "the form carries the token");
});
