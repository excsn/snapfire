import { get } from "@snapfire/fsr-client/store";
import { describe, expect, fireEvent, load, settle, spyOn, test } from "@snapfire/fsr-client/testing";

import { owner, slotOwner } from "@src/store";

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

  test("two streamed slots seeding one key settle it by position, whichever resolved first", async () => {
    await load("/streamed");
    await settle();
    expect(document.querySelector(".slot-alpha")?.textContent).toEqual("alpha");
    expect(document.querySelector(".slot-beta")?.textContent).toEqual("beta");
    expect(get(slotOwner), "the later slot name wins at one depth").toEqual("beta");
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
