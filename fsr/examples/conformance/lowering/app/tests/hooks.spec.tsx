import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the hooks page", () => {
  test("renders custom hooks, each call with state of its own, and a reducer's initial state", async () => {
    await load("/hooks");
    await settle();
    expect(text(".apples")).toEqual("1 2");
    expect(text(".pears")).toEqual("10 20");
    expect(text(".open")).toEqual("shut");
    expect(text(".tally")).toEqual("3 ");
  });

  test("renders again a component that sets its own state while rendering as React does and hydrates over it", async () => {
    const html = await (await fetch("/hooks")).text();
    expect(html, "the server ran the body until no setter fired").toContain('<p class="direction">The count is <!-- -->increasing</p>');
    await load("/hooks");
    await settle();
    const direction = document.querySelector(".trend .direction");
    expect(direction?.textContent).toEqual("The count is increasing");
    await fireEvent.click(document.querySelector(".trend .less")!);
    await settle();
    expect(text(".trend .count")).toEqual("2");
    expect(document.querySelector(".trend .direction"), "React hydrated the server's element and patched it").toBe(direction);
    expect(direction?.textContent).toEqual("The count is decreasing");
  });

  test("hydrates the handlers the hooks returned and the reducer's dispatch", async () => {
    await load("/hooks");
    await settle();
    await fireEvent.click(document.querySelector(".pear")!);
    await fireEvent.click(document.querySelector(".toggle")!);
    await fireEvent.click(document.querySelector(".add")!);
    await settle();
    expect(text(".pears")).toEqual("15 30");
    expect(text(".apples")).toEqual("1 2");
    expect(text(".open")).toEqual("open");
    expect(text(".tally")).toEqual("4 add");
  });

  test("reads a provider's value inline and inside an island of its own framework placed under it", async () => {
    await load("/hooks");
    await settle();
    expect(text(".label.inline")).toEqual("inline:dark");
    expect(text(".label.island")).toEqual("island:dark");
    await fireEvent.click(document.querySelector(".flip")!);
    await settle();
    expect(text(".label.inline")).toEqual("inline:light");
    expect(text(".label.island")).toEqual("island:light");
  });

  test("spells each useId from the component and its placement, the same on both halves", async () => {
    await load("/hooks");
    await settle();
    const first = document.querySelector(".first")!.getAttribute("for")!;
    expect(first).toEqual("_S_src/ui/Fields.tsx#default|0_");
    expect(document.getElementById(first)?.getAttribute("aria-describedby")).toEqual("_S_src/ui/Fields.tsx#default|1_");
    expect(document.getElementById("_S_src/ui/Fields.tsx#default|1_")?.getAttribute("class")).toEqual("hint");
    const fields = [...document.querySelectorAll(".field label")].map((label) => label.getAttribute("for")!);
    expect(fields).toEqual(["_S_src/ui/Fields.tsx#Field|0@0.c0_", "_S_src/ui/Fields.tsx#Field|0@1.c0_"]);
    expect(fields.map((id) => document.getElementById(id)?.tagName)).toEqual(["INPUT", "INPUT"]);
    await fireEvent.click(document.querySelector(".show")!);
    await settle();
    expect(text(".hint")).toEqual("shown");
    expect(document.querySelector(".first")!.getAttribute("for")).toEqual("_S_src/ui/Fields.tsx#default|0_");
  });

  test("reads a provider's value through use inside an expression", async () => {
    await load("/hooks");
    await settle();
    expect(text(".tag")).toEqual("DARK");
    await fireEvent.click(document.querySelector(".flip")!);
    await settle();
    expect(text(".tag")).toEqual("LIGHT");
  });
});
