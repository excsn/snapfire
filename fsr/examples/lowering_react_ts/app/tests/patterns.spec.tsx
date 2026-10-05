import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the patterns page", () => {
  test("renders what its loader destructured", async () => {
    await load("/patterns");
    await settle();
    expect(text(".who")).toEqual("A-17 for Ada in Lyon, FR");
    expect(text(".ends")).toEqual("pear plum");
    expect([...document.querySelectorAll(".lines li")].map((li) => li.textContent)).toEqual(["PEAR 6", "FIG 1.5", "PLUM 0"]);
  });

  test("hydrates an island whose props destructure with renames and defaults", async () => {
    await load("/patterns");
    await settle();
    expect(text(".summary")).toEqual("2 pear at 3 each = 6");
    await fireEvent.click(document.querySelector(".more")!);
    await settle();
    expect(text(".summary")).toEqual("3 pear at 3 each = 9");
  });
});
