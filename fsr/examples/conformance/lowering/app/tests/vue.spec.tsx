import { describe, expect, fireEvent, load, settle, test, userEvent } from "@snapfire/fsr-client/testing";

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

  test("binds a child's defineModel to the parent through v-model", async () => {
    await load("/vue");
    await settle();
    expect((document.querySelector(".query") as HTMLInputElement).value).toEqual("");
    await userEvent.type(document.querySelector(".query")!, "fi");
    await settle();
    expect(texts(".tile .name")).toEqual(["fig"]);
  });

  test("fills a named slot with the props the child hands it, falls back where it is not given and reads what the parent provides", async () => {
    await load("/vue");
    await settle();
    expect(texts(".tile .badge")).toEqual(["3 left"]);
    expect(texts(".tile .shelf")).toEqual(["pantry", "pantry"]);
    expect([...document.querySelectorAll(".tile")].map((t) => t.getAttribute("data-name"))).toEqual(["pear", "fig"]);
    await fireEvent.click(document.querySelectorAll(".take")[0]!);
    await settle();
    expect(texts(".tile .badge")).toEqual(["2 left"]);
    expect(document.querySelector(".tile")?.getAttribute("data-left")).toEqual("2");
  });

  test("writes a bound attribute whose name is an expression and moves it when the name changes", async () => {
    await load("/vue");
    await settle();
    const tiles = [...document.querySelectorAll(".tile")];
    expect(tiles.map((t) => t.getAttribute("data-plenty"))).toEqual(["pantry", null]);
    expect(tiles.map((t) => t.getAttribute("data-low"))).toEqual([null, "pantry"]);
    await fireEvent.click(document.querySelectorAll(".take")[0]!);
    await settle();
    expect(tiles[0]!.getAttribute("data-plenty")).toEqual(null);
    expect(tiles[0]!.getAttribute("data-low")).toEqual("pantry");
  });

  test("stamps slot content for the child's :slotted() rules the way Vue does", async () => {
    await load("/vue");
    await settle();
    const tile = document.querySelector(".tile")!;
    const own = [...tile.attributes].map((a) => a.name).find((n) => /^data-v-[0-9a-f]+$/.test(n) && !document.querySelector("ul.shelf")?.hasAttribute(n));
    expect(own, "the tile carries its own scope").toBeTruthy();
    const note = tile.querySelector(".note")!;
    expect(note.hasAttribute(`${own}-s`), `the note carries ${own}-s`).toBeTruthy();
    expect([...document.querySelectorAll(".badge")].every((b) => b.hasAttribute(`${own}-s`)), "named slot content too").toBeTruthy();
    expect(tile.querySelector(".name")!.hasAttribute(`${own}-s`), "the tile's own elements do not").toBeFalsy();
  });
});
