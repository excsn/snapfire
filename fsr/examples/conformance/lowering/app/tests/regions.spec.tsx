import { navigate } from "@snapfire/fsr-client";
import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const texts = (selector: string) => Array.from(document.querySelectorAll(selector)).map((el) => el.textContent);

describe("the regions page", () => {
  test("hydrates an island whose props hold markup, the server's markup for each adopted where the island renders it", async () => {
    await load("/regions");
    await settle();
    expect(texts(".label")).toEqual(["Questions, answered"]);
    expect(texts(".label em")).toEqual(["answered"]);
    expect(texts(".body .answer")).toEqual(["Because"]);
    expect(document.querySelectorAll("template[data-sf-m]").length).toBe(0);
    await fireEvent.click(document.querySelectorAll(".toggle")[1]!);
    await settle();
    expect(texts(".body .answer")).toEqual(["Like so"]);
  });

  test("sends a markup prop the island renders once, in place", async () => {
    const html = await (await fetch("/regions")).text();
    expect(html.split("<p class=\"answer\">Because</p>").length - 1, "the open answer is placed, not also held").toBe(1);
    expect(html.split("<p class=\"answer\">Like so</p>").length - 1, "the closed answer is held for when it opens").toBe(1);
  });

  test("a navigation that changes the markup props patches the island in place, its state kept", async () => {
    await load("/regions");
    await settle();
    const island = document.querySelector("sf-i");
    await fireEvent.click(document.querySelectorAll(".toggle")[1]!);
    await settle();
    expect(texts(".body .answer")).toEqual(["Like so"]);
    await navigate("/regions?set=b");
    await settle();
    expect(document.querySelector("sf-i"), "the island was patched, not mounted again").toBe(island);
    expect(texts(".label")).toEqual(["Questions, revisited"]);
    expect(texts(".body .answer"), "the second item is still the open one, showing the new page's markup").toEqual(["Another way"]);
    await fireEvent.click(document.querySelectorAll(".toggle")[0]!);
    await settle();
    expect(texts(".body .answer"), "a panel opened after the patch shows the new page's markup").toEqual(["Still because"]);
  });
});
