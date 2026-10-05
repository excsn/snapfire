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
});
