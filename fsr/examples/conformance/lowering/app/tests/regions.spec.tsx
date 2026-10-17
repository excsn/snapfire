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
});
