import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;
const badges = () => [...document.querySelectorAll(".badge")].map((b) => b.textContent);

describe("the statements page", () => {
  test("renders what the loader accumulated in its loop", async () => {
    await load("/statements");
    await settle();
    expect(text(".headline")).toEqual("51 in stock, 2 low");
    expect(text(".tone")).toEqual("urgent");
    expect(text(".kinds")).toEqual("fresh pear, fresh fig, fresh leek, dry salt");
    expect(text(".chosen")).toEqual("pear");
  });

  test("takes the early return and the lets after it read the query", async () => {
    await load("/statements?pick=none");
    expect(text(".empty")).toEqual("nothing picked");
    await load("/statements?pick=leek");
    expect(text(".chosen")).toEqual("leek");
  });

  test("hydrates islands whose bodies return from if, else and switch", async () => {
    await load("/statements");
    await settle();
    expect(badges()).toEqual(["out", "two left", "9 veg", "plenty"]);
    await fireEvent.click(document.querySelector(".badge.low")!);
    await settle();
    expect(badges()).toEqual(["out", "one left", "9 veg", "plenty"]);
    await fireEvent.click(document.querySelector(".badge.low")!);
    await settle();
    expect(badges()).toEqual(["out", "out", "9 veg", "plenty"]);
  });

  test("renders an island whose map callback writes its locals and whose markup comes from functions it declares", async () => {
    await load("/statements");
    const skus = () => [...document.querySelectorAll(".share .sku")].map((s) => `${s.tagName.toLowerCase()}:${s.textContent}`);
    expect(skus()).toEqual(["span:pear", "span:fig", "span:leek", "span:salt"]);
    expect([...document.querySelectorAll(".share .bar")].map((b) => b.getAttribute("data-share"))).toEqual(["0", "4", "18", "78"]);
    expect([...document.querySelectorAll(".share .note")].map((n) => n.textContent)).toEqual(["out", "low"]);
    await settle();
    await fireEvent.click(document.querySelectorAll(".pick")[2]!);
    await settle();
    expect(skus()).toEqual(["span:pear", "span:fig", "b:LEEK", "span:salt"]);
    expect(document.querySelectorAll(".share .bar.hot").length).toEqual(1);
  });
});
