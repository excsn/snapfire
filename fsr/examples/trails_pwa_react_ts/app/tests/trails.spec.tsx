import { ctx, expect, load, test } from "@snapfire/fsr/client/testing";
import { set, online, updateReady } from "@snapfire/fsr/client";

test("the list links every trail and the status says nothing while connected", async () => {
  await load("/", { ctx: ctx() });
  expect(document.querySelectorAll(".list li").length).toEqual(4);
  expect(document.querySelector(".list a")?.getAttribute("href")).toEqual("/trail/ridge");
  expect(document.querySelector(".status")?.textContent).toEqual("");
});

test("the status says so when the connection goes and when a new version is ready", async () => {
  await load("/trail/chalk", { ctx: ctx() });
  expect(document.querySelector("h1")?.textContent).toEqual("Chalk Figure Walk");
  set(online, false);
  set(updateReady, true);
  await Promise.resolve();
  expect(document.querySelector(".offline")?.textContent).toEqual("Offline: you are reading saved pages.");
  expect(document.querySelector(".update a")?.getAttribute("href")).toEqual("/");
});

test("opening the new version is left to the browser, a full load of the new build", async () => {
  await load("/trail/chalk", { ctx: ctx() });
  set(updateReady, true);
  await Promise.resolve();
  const click = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
  document.querySelector(".update a")?.dispatchEvent(click);
  expect(click.defaultPrevented).toEqual(false);
});

test("the offline page links back to the trails", async () => {
  await load("/offline", { ctx: ctx() });
  expect(document.querySelector("h1")?.textContent).toEqual("No signal");
});
