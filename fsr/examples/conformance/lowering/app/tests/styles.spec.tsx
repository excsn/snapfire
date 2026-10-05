import { describe, expect, load, settle, test } from "@snapfire/fsr-client/testing";

describe("the styles page", () => {
  test("renders each bar's width and opacity as a style attribute", async () => {
    await load("/styles");
    await settle();
    const fills = [...document.querySelectorAll(".bar-fill")].map((el) => el.getAttribute("style"));
    expect(fills).toEqual(["width:40%;opacity:0.6", "width:100%;opacity:1", "width:80%;opacity:0.6"]);
    expect(document.querySelector(".bar-value")?.getAttribute("style")).toEqual("margin-left:8px");
  });
});
