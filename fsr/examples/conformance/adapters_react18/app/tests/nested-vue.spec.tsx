import { advance, ctx, expect, fireEvent, load, settle, spyOn, test } from "@snapfire/fsr/client/testing";


const finishPantry = async () => {
  (globalThis as { finishPantry?: () => void }).finishPantry?.();
  await advance(1000);
};

test("a Vue island among an island's children reads what it provides and follows it", async () => {
  const warned = spyOn(console, "warn");
  await load("/nested/vue", { ctx: ctx({}) });
  await settle();
  await finishPantry();
  expect(document.querySelector(".servings")?.textContent, "the server rendered it in the shell's frame").toEqual("Serves 2");
  const mismatches = warned.mock.calls.filter((call) => String(call[0]).includes("Hydration"));
  expect(mismatches, "hydration found what the server wrote").toEqual([]);
  await fireEvent.click(document.querySelector(".plan-four") as Element);
  await settle();
  expect(document.querySelector(".servings")?.textContent).toEqual("Serves 4");
  warned.mockRestore();
});

test("a throw in a Vue island among the children reaches the handler around it and a retry renders it again", async () => {
  const reported = spyOn(console, "error").mockImplementation(() => {});
  const warned = spyOn(console, "warn").mockImplementation(() => {});
  await load("/nested/vue", { ctx: ctx({}) });
  await settle();
  await finishPantry();
  await fireEvent.click(document.querySelector(".shopping") as Element);
  await settle();
  expect(document.querySelector(".plan-failed"), "the boundary caught it").toBeTruthy();
  await fireEvent.click(document.querySelector(".plan-retry") as Element);
  await settle();
  expect(document.querySelector(".shopping")?.textContent).toEqual("Build the list");
  reported.mockRestore();
  warned.mockRestore();
});

test("a Vue island among the children holding an async child keeps the Suspense around it pending until it resolves", async () => {
  await load("/nested/vue", { ctx: ctx({}) });
  await settle();
  expect(document.querySelector(".plan-loading"), "the boundary's fallback").toBeTruthy();
  await finishPantry();
  expect(document.querySelector(".plan-loading")).toBeNull();
  expect(document.querySelector(".pantry-count")?.textContent).toEqual("12 in the pantry");
});
