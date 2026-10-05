import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the regex page", () => {
  test("renders slugs, title case and matches the loader and page computed", async () => {
    await load("/regex");
    await settle();
    const links = [...document.querySelectorAll(".entries a")];
    expect(links.map((a) => a.getAttribute("href"))).toEqual(["/regex/pears-figs-a-field-guide", "/regex/leeks-onions-and-other-alliums", "/regex/salt-coarse-vs-fine"]);
    expect(links.map((a) => a.textContent)).toEqual(["Pears & Figs: A Field Guide", "Leeks, Onions And Other Alliums", "Salt (Coarse) Vs. Fine"]);
    expect(text(".years")).toEqual("1998 → 2004 → 2019");
    expect(text(".words")).toEqual("5 words");
    expect(text(".hits")).toEqual("1 with and");
  });

  test("hydrates an island that filters with a regular expression", async () => {
    await load("/regex?q=salt");
    await settle();
    expect(text(".hits")).toEqual("1 with salt");
    expect(text(".shown")).toEqual("Leeks Onions and Other Alliums");
    await fireEvent.click(document.querySelector(".toggle")!);
    await settle();
    expect(text(".shown")).toEqual("Onions and Other Alliums");
  });
});
