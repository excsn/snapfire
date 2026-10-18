import { describe, expect, load, test } from "@snapfire/fsr-client/testing";

const text = (selector: string) => document.querySelector(selector)?.textContent;

describe("the denied page", () => {
  test("a loader's forbidden renders its own boundary and answers 403", async () => {
    const { status } = await load("/denied?as=forbidden");
    expect(status).toEqual(403);
    expect(text(".forbidden h1")).toEqual("Not for you");
    expect(text(".forbidden .why")).toEqual("forbidden: data source denied: this shelf is staff only");
  });

  test("a kind with no boundary of its own falls back to error.tsx with its status", async () => {
    const { status } = await load("/denied?as=busy");
    expect(status).toEqual(429);
    expect(text(".failed .why")).toEqual("too_many_requests: data source denied: too many visits, try again shortly");
  });

  test("the page renders when the loader does not fail", async () => {
    const { status } = await load("/denied");
    expect(status).toEqual(200);
    expect(text(".shelf")).toEqual("open");
  });
});
