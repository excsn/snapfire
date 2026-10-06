import { contribute, get, isPending, reset, retain, whenSettled } from "@snapfire/fsr-client/store";
import { describe, expect, fireEvent, load, render, settle, spyOn, test } from "@snapfire/fsr-client/testing";

import { owner, slotOwner } from "@src/store";
import Owner from "@src/ui/Owner";

const owners = () => [...document.querySelectorAll(".owner")].map((el) => `${(el as HTMLElement).dataset.island}=${el.textContent}`);

describe("a streamed page that seeds a key its layout also seeds", () => {
  test("hydrates the layout's islands against the value they were rendered with, then follows the page's", async () => {
    const errors = spyOn(console, "error");
    const warnings = spyOn(console, "warn");
    await load("/streamed");
    await settle();
    expect(errors.mock.calls.map((call) => String(call[0]).slice(0, 120)), "no hydration mismatch").toEqual([]);
    expect(warnings.mock.calls.map((call) => String(call[0]).slice(0, 120)), "no hydration warning").toEqual([]);
    expect(owners()).toEqual(["eager=page", "vue-eager=page", "page-react=page", "page-vue=page", "lazy=page", "vue-lazy=page"]);
    errors.mockRestore();
    warnings.mockRestore();
  });

  test("hydrates clean when both seeds agree", async () => {
    const errors = spyOn(console, "error");
    const warnings = spyOn(console, "warn");
    await load("/streamed?page=layout");
    await settle();
    expect(errors.mock.calls.length).toEqual(0);
    expect(warnings.mock.calls.length).toEqual(0);
    expect(owners()).toEqual(["eager=layout", "vue-eager=layout", "page-react=layout", "page-vue=layout", "lazy=layout", "vue-lazy=layout"]);
    errors.mockRestore();
    warnings.mockRestore();
  });

  test("two streamed slots seeding one key settle it by the configured slot order, whichever resolved first", async () => {
    await load("/streamed");
    await settle();
    expect(document.querySelector(".slot-alpha")?.textContent).toEqual("alpha");
    expect(document.querySelector(".slot-beta")?.textContent).toEqual("beta");
    expect(document.documentElement.getAttribute("data-sf-slot-order"), "the document carries [store] slot_order").toEqual('["beta","alpha"]');
    expect(get(slotOwner), "the later name in the order wins; by name alone beta would").toEqual("alpha");
  });

  test("a segment's seed leaves with the segment", async () => {
    await load("/streamed/other");
    await settle();
    expect(get(owner)).toEqual("other");
    await fireEvent.click(document.querySelector("a.brand")!);
    await settle();
    expect(location.pathname).toEqual("/");
    expect(get(owner), "the page that seeded it is gone and nothing else seeds it").toBeUndefined();
  });

  test("an intercept's seed wins while it is open and goes when it closes", async () => {
    await load("/streamed?page=layout");
    await settle();
    expect(owners()).toEqual(["eager=layout", "vue-eager=layout", "page-react=layout", "page-vue=layout", "lazy=layout", "vue-lazy=layout"]);
    await fireEvent.click(document.querySelector("a.to-other")!);
    await settle();
    expect(location.pathname).toEqual("/streamed/other");
    expect(document.querySelector(".peek-by")?.textContent, "the intercept opened in the peek slot").toEqual("other");
    expect(document.querySelector(".page-by")?.textContent, "over the page, which stayed").toEqual("layout");
    expect(get(owner), "the intercept's seed wins the key it shares with the page beneath").toEqual("other");
    expect(owners().every((o) => o.endsWith("=other")), owners().join(" ")).toBeTruthy();
    await fireEvent.click(document.querySelector("a.peek-close")!);
    await settle();
    expect(document.querySelector(".peek-by"), "the intercept closed").toBeNull();
    expect(get(owner), "and its seed left with it").toEqual("layout");
    expect(owners().every((o) => o.endsWith("=layout")), owners().join(" ")).toBeTruthy();
  });
});

describe("a key a streamed segment has yet to seed", () => {
  test("holds a layout island that reads it out of the first wave, so the first paint never shows the value about to be replaced", async () => {
    const html = await (await fetch("/streamed")).text();
    const wave = html.slice(0, html.indexOf("<template data-sf-fill"));
    expect(wave.includes('class="owner"'), "no island reading the key renders in the first wave").toBeFalsy();
    expect(wave.includes('"$aw":["repro/owner"]'), "each says which key it waits for").toBeTruthy();
    expect(wave.includes('"w":["repro/owner"]'), "the document says which segment will seed it").toBeTruthy();
  });

  test("promises the keys a store export returns through a spread", async () => {
    const html = await (await fetch("/streamed")).text();
    const wave = html.slice(0, html.indexOf("<template data-sf-fill"));
    expect(wave.includes('{"k":"routes/streamed/slots/beta/page.tsx#default","p":["content","content","beta"],"v":{},"w":["repro/slot"]}'), "beta's spread store still says which key it will seed").toBeTruthy();
  });

  test("is pending while a promise outranks what the store holds, until the seed keeps it", async () => {
    reset();
    contribute([{ k: "layout", p: [], v: { "repro/owner": "layout" } }]);
    contribute([{ k: "page", p: ["content"], v: {}, w: ["repro/owner"] }]);
    expect(isPending(owner)).toBeTruthy();
    expect(get(owner), "the value held meanwhile is the layout's").toEqual("layout");
    let settled = false;
    void whenSettled([owner]).then(() => (settled = true));
    contribute([{ k: "page", p: ["content"], v: { "repro/owner": "page" } }]);
    await settle();
    expect(settled).toBeTruthy();
    expect(isPending(owner)).toBeFalsy();
    expect(get(owner)).toEqual("page");
  });

  test("keeps what a segment last seeded while a new promise for it is out", async () => {
    reset();
    contribute([{ k: "page", p: ["content"], v: { "repro/owner": "before" } }]);
    contribute([{ k: "page", p: ["content"], v: {}, w: ["repro/owner"] }]);
    expect(isPending(owner)).toBeTruthy();
    expect(get(owner), "a mounted island goes on showing it until the seed lands").toEqual("before");
  });

  test("keeps what the segment a navigation is replacing seeded, at the same slot path", async () => {
    reset();
    contribute([{ k: "layout", p: [], v: { "repro/owner": "layout" } }, { k: "page?page=a", p: ["content"], v: { "repro/owner": "a" } }]);
    contribute([{ k: "page?page=b", p: ["content"], v: {}, w: ["repro/owner"] }]);
    retain(["layout", "page?page=b"]);
    expect(isPending(owner)).toBeTruthy();
    expect(get(owner), "not the layout's value: the page it replaces held the slot").toEqual("a");
    contribute([{ k: "page?page=b", p: ["content"], v: { "repro/owner": "b" } }]);
    expect(get(owner)).toEqual("b");
  });

  test("suspends a root the server did not render until the seed lands while a mounted one keeps its value", async () => {
    reset();
    contribute([{ k: "layout", p: [], v: { "repro/owner": "layout" } }, { k: "page", p: ["content"], v: {}, w: ["repro/owner"] }]);
    const view = await render(<Owner name="fresh" />, { hydrate: false });
    expect(view.container.querySelector(".owner"), "nothing shows while the key is pending").toBeNull();
    contribute([{ k: "page", p: ["content"], v: { "repro/owner": "page" } }]);
    await settle();
    expect(view.container.querySelector(".owner")?.textContent).toEqual("page");
    contribute([{ k: "page", p: ["content"], v: {}, w: ["repro/owner"] }]);
    await settle();
    expect(view.container.querySelector(".owner")?.textContent, "a promise for a mounted root's key leaves what it shows").toEqual("page");
    view.unmount();
  });
});
