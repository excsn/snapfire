import { ctx, expect, fireEvent, load, settle, spyOn, test } from "@snapfire/fsr-client/testing";

test("a Svelte island among an island's children reads the context it sets and follows it", async () => {
  const warned = spyOn(console, "warn");
  await load("/nested/svelte", { ctx: ctx({}) });
  await settle();
  expect(document.querySelector(".portions")?.textContent, "the server rendered it in the kitchen's frame").toEqual("Serves 2");
  const mismatches = warned.mock.calls.filter((call) => String(call[0]).includes("hydration"));
  expect(mismatches, "hydration found what the server wrote").toEqual([]);
  await fireEvent.click(document.querySelector(".kitchen-four") as Element);
  await settle();
  expect(document.querySelector(".portions")?.textContent).toEqual("Serves 4");
  expect(document.querySelector(".kitchen-note")?.textContent, "the markup beside the islands is kept").toEqual("From the pantry");
  warned.mockRestore();
});

test("a Svelte island among the children that renders in the browser alone joins the tree and reads the context", async () => {
  await load("/nested/svelte", { ctx: ctx({}) });
  await settle();
  expect(document.querySelector(".timer")?.textContent).toEqual("Timing for 2");
  await fireEvent.click(document.querySelector(".kitchen-four") as Element);
  await settle();
  expect(document.querySelector(".timer")?.textContent).toEqual("Timing for 4");
});

test("a throw in a Svelte island among the children reaches the boundary around it and a reset renders it again", async () => {
  const reported = spyOn(console, "error").mockImplementation(() => {});
  const warned = spyOn(console, "warn").mockImplementation(() => {});
  await load("/nested/svelte", { ctx: ctx({}) });
  await settle();
  await fireEvent.click(document.querySelector(".basket") as Element);
  await settle();
  expect(document.querySelector(".kitchen-failed"), "the boundary caught it").toBeTruthy();
  await fireEvent.click(document.querySelector(".kitchen-retry") as Element);
  await settle();
  expect(document.querySelector(".basket")?.textContent).toEqual("Fill the basket");
  reported.mockRestore();
  warned.mockRestore();
});

test("a Svelte island timed on visible among the children keeps the markup the server rendered in the kitchen's frame", async () => {
  await load("/nested/svelte", { ctx: ctx({}) });
  await settle();
  expect(document.querySelector(".remains")?.textContent).toEqual("Remains for 2");
});
