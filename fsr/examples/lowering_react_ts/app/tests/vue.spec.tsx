import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const texts = (selector: string) => [...document.querySelectorAll(selector)].map((e) => e.textContent);

describe("the vue page", () => {
  test("renders a Vue island's children with their props and slot content", async () => {
    await load("/vue");
    await settle();
    expect(texts(".tile .name")).toEqual(["pear", "fig"]);
    expect(texts(".tile .left")).toEqual(["3", "1"]);
    expect(texts(".tile .note")).toEqual(["plenty", "low"]);
  });

  test("hydrates the children, each with state of its own", async () => {
    await load("/vue");
    await settle();
    await fireEvent.click(document.querySelectorAll(".take")[1]!);
    await settle();
    expect(texts(".tile .left")).toEqual(["3", "0"]);
  });
});
