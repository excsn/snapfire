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

  test("reads a provider's value inline and the default inside an island of its own", async () => {
    await load("/hooks");
    await settle();
    expect(text(".label.inline")).toEqual("inline:dark");
    expect(text(".label.island")).toEqual("island:light");
    await fireEvent.click(document.querySelector(".flip")!);
    await settle();
    expect(text(".label.inline")).toEqual("inline:light");
  });
});
