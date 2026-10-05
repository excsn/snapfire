import { ctx, describe, expect, fireEvent, load, render, settle, test, waitFor } from "@snapfire/fsr-client/testing";

import CtxAction from "@src/ui/CtxAction";

const stamp = () => ({ stamp: "spec" });

const request = () => ctx({ locale: "fr", session: { note: "held" }, identity: { subject: "ada" }, services: { probe: { getStamp: stamp } } });

const text = (selector: string) => document.querySelector(selector)?.textContent;

const calls = (c: ReturnType<typeof ctx>) => c.trace.calls.filter((call) => call.service === "probe" && call.method === "getStamp").length;

const answer = async () => {
  await waitFor(() => expect(text(".answer")).not.toEqual(""));
  return JSON.parse(text(".answer")!) as { note: string; subject: string; locale: string; stamp: string };
};

describe("load()", () => {
  test("renders the page under the ctx's locale, session, identity and services", async () => {
    const c = request();
    await load("/ctx/a?q=1", { ctx: c });
    await settle();
    expect(text(".locale")).toEqual("fr");
    expect(text(".note")).toEqual("held");
    expect(text(".subject")).toEqual("ada");
    expect(text(".stamp")).toEqual("spec");
    expect(calls(c), "the loader's call is in the ctx's trace").toEqual(1);
  });

  test("gives the loader the path, params and query the URL names", async () => {
    await load("/ctx/a?q=1", { ctx: request() });
    expect(text(".id")).toEqual("a");
    expect(text(".q")).toEqual("1");
    expect(text(".path")).toEqual("/ctx/a");
  });

  test("hands its islands the ctx's locale", async () => {
    await load("/ctx/a", { ctx: request() });
    await settle();
    expect(text(".island-locale")).toEqual("fr");
    expect(text(".step-locale")).toEqual("fr");
  });
});

describe("a navigation", () => {
  test("loads the next page under the same ctx with the next URL", async () => {
    const c = request();
    await load("/ctx/a?q=1", { ctx: c });
    await settle();
    await fireEvent.click(document.querySelector("a.other")!);
    await settle();
    expect(location.pathname).toEqual("/ctx/b");
    expect(text(".id")).toEqual("b");
    expect(text(".q")).toEqual("2");
    expect(text(".path")).toEqual("/ctx/b");
    expect(text(".locale")).toEqual("fr");
    expect(text(".note")).toEqual("held");
    expect(text(".subject")).toEqual("ada");
    expect(text(".stamp")).toEqual("spec");
    expect(calls(c), "both loaders called the ctx's service").toEqual(2);
  });
});

describe("an action an island calls", () => {
  test("runs under the ctx's session, identity, locale and services and writes the session back", async () => {
    const c = request();
    await load("/ctx/a", { ctx: c });
    await settle();
    await fireEvent.click(document.querySelector(".touch")!);
    expect(await answer()).toEqual({ note: "from react", subject: "ada", locale: "fr", stamp: "spec" });
    expect(c.session.note, "the write reached the ctx").toEqual("from react");
    expect(calls(c), "the loader's, the action's and the loader's again when the page refreshes after it").toEqual(3);
    expect(text(".note"), "the refreshed page reads the session the action wrote").toEqual("from react");
  });
});

describe("a server island's step", () => {
  test("renders under the ctx's locale and dispatches its action under the ctx", async () => {
    const c = request();
    await load("/ctx/a", { ctx: c });
    await settle();
    await fireEvent.click(document.querySelector(".step")!);
    await settle();
    expect(text(".steps")).toEqual("1");
    expect(text(".step-locale"), "the step's render").toEqual("fr");
    expect(c.session.note, "the action the handler called").toEqual("from the server");
    expect(calls(c), "the loader's, the action's and the loader's again when the page refreshes after it").toEqual(3);
    expect(text(".note"), "the refreshed page reads the session the action wrote").toEqual("from the server");
  });
});

describe("render()", () => {
  test("mounts an island under the ctx's locale and its action runs under the ctx", async () => {
    const c = request();
    const r = await render(<CtxAction />, { ctx: c });
    expect(r.container.querySelector(".island-locale")?.textContent).toEqual("fr");
    await fireEvent.click(r.getByText("touch"));
    expect(await answer()).toEqual({ note: "from react", subject: "ada", locale: "fr", stamp: "spec" });
    expect(c.session.note).toEqual("from react");
  });
});
