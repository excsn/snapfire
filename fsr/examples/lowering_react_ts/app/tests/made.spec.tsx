import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the made page", () => {
  test("renders what the loader made with Date, Map, Set and URLSearchParams", async () => {
    await load("/made");
    await settle();
    expect(text(".kinds")).toEqual("fruit veg (2)");
    expect(text(".totals")).toEqual("fruit=8 veg=2");
    expect(text(".iso")).toEqual("2026-10-03T17:05:00.000Z");
    expect(text(".past")).toEqual("delivered");
    expect(document.querySelector(".link")?.getAttribute("href")).toEqual("/made?kind=fruit&q=tea+%26+cake");
  });

  test("formats with Intl under the request's locale", async () => {
    await load("/made");
    expect(text(".day")).toEqual("Oct 3, 2026");
    expect(text(".long")).toEqual("October 3, 2026");
    expect(text(".share")).toEqual("4");
  });

  test("hydrates an island that does date arithmetic in UTC", async () => {
    await load("/made");
    await settle();
    expect(text(".when")).toEqual("Sat 2026-10-03");
    expect(text(".seen")).toEqual("1");
    await fireEvent.click(document.querySelector(".later")!);
    await settle();
    expect(text(".when")).toEqual("Sun 2026-10-04");
    expect(text(".seen")).toEqual("2");
  });
});
