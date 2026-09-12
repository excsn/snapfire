import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "Two batteries." };
const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden"] };

const stocked = (session = {}) =>
  ctx({
    session,
    services: { shed: { getShed: () => shed, listTools: () => [trimmer], listLoans: () => [], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
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

test("nothing on the page mounts: no island, no framework", async () => {
  await load("/tool/1", { ctx: stocked() });
  assert.equal(document.querySelectorAll("sf-i").length, 0);
  assert.ok(document.querySelector("loan-planner template[shadowrootmode=open]"), "the planner's shadow root is written by the server");
  assert.ok(document.querySelector("form.reserve input[name=_csrf]"), "the form carries the token");
});
