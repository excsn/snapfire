import { describe, expect, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the methods page", () => {
  test("renders what its loader computed with the array and string methods", async () => {
    await load("/methods");
    await settle();
    expect(text(".cheapest")).toEqual("plum, fig, pear");
    expect(text(".tags")).toEqual("fruit fuzzy green");
    expect(text(".priciest")).toEqual("3.00");
    expect(text(".last")).toEqual("plum");
    expect(text(".codes")).toEqual("PE--|FI--|KI--|PL--|END");
    expect(text(".json")).toEqual('{"take":3,"first":"plum"}');
    expect(text(".squared")).toEqual("9");
    expect(text(".reversed")).toEqual("fuzzy green fruit");
  });

  test("reads the query the URL names", async () => {
    await load("/methods?take=2");
    expect(text(".cheapest")).toEqual("plum, fig");
    expect(text(".squared")).toEqual("4");
  });

  test("hydrates an island whose render calls them and keeps computing them in the browser", async () => {
    await load("/methods");
    await settle();
    expect(text(".shown")).toEqual("plum > fig > pear");
    expect(text(".label")).toEqual("..plum");
    expect(text(".position")).toEqual("1 of 3");
    await fireEvent.click(document.querySelector(".next")!);
    await settle();
    expect(text(".shown")).toEqual("fig > pear > plum");
    expect(text(".label")).toEqual("...fig");
    expect(text(".position")).toEqual("2 of 3");
  });
});
