import { enableNavigation } from "@snapfire/fsr-client";
import { ctx, expect, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

interface Entry {
  target: Element;
  isIntersecting: boolean;
}

/** linkedom has no IntersectionObserver, so the spec is the one that decides when a link comes into view. */
function observeInstead(): { observed: Element[]; enter: (el: Element) => void } {
  const observed: Element[] = [];
  let callback: ((entries: Entry[]) => void) | null = null;
  class Stub {
    constructor(cb: (entries: Entry[]) => void) {
      callback = cb;
    }
    observe(el: Element): void {
      observed.push(el);
    }
    unobserve(el: Element): void {
      const at = observed.indexOf(el);
      if (at !== -1) observed.splice(at, 1);
    }
    disconnect(): void {}
  }
  (globalThis as { IntersectionObserver?: unknown }).IntersectionObserver = Stub;
  return { observed, enter: (el) => callback?.([{ target: el, isIntersecting: true }]) };
}

test("a link is prefetched as it enters the viewport under that timing, and once only", async () => {
  const view = observeInstead();
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  enableNavigation({ prefetch: "viewport" });

  const cart = screen.getByLabelText("Cart, 2 items");
  expect(view.observed.includes(cart), `every link is observed under viewport timing; observed ${view.observed.length}`).toBeTruthy();

  const before = c.trace.calls.length;
  view.enter(cart);
  await settle();
  expect(c.trace.calls.length > before, "the cart's loader ran for the prefetch").toBeTruthy();
  expect(view.observed.includes(cart), "a link that has been prefetched is dropped").toEqual(false);

  const after = c.trace.calls.length;
  view.enter(cart);
  await settle();
  expect(c.trace.calls.length, "and the payload it holds answers the next look without another fetch").toEqual(after);
});

test("hover timing leaves a link unobserved", async () => {
  const view = observeInstead();
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  enableNavigation({ prefetch: "hover" });
  expect(view.observed.length, "nothing is watched while the document prefetches on hover").toEqual(0);
});
