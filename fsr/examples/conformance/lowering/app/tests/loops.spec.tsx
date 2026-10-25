import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr/client/testing";

const texts = (selector: string) => Array.from(document.querySelectorAll(selector)).map((el) => el.textContent);

describe("the loops page", () => {
  test("hydrates an island whose for…of and counted for build its rows from what the server computed per row", async () => {
    await load("/loops");
    await settle();
    expect(texts(".looped li")).toEqual(["1.3", "2.5"]);
    expect(texts(".mapped li")).toEqual(["1.3", "2.5"]);
    expect(texts(".counted li")).toEqual(["2.5", "5"]);
    await fireEvent.click(document.querySelector(".pick")!);
    await settle();
    expect(texts(".looped li.picked")).toEqual(["1.3"]);
    expect(texts(".counted li.picked")).toEqual(["2.5"]);
    expect(texts(".looped li")).toEqual(["1.3", "2.5"]);
  });
});
