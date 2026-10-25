import { applyHead, loadScript } from "@snapfire/fsr/client";
import { expect, settle, test } from "@snapfire/fsr/client/testing";

const tags = (name: string) => Array.from(document.querySelectorAll<HTMLScriptElement>("script[src]")).filter((s) => s.src.endsWith(name));

test("a script is added once however often it is asked for and resolves once it has loaded", async () => {
  let done = false;
  const first = loadScript("/static/demo/once.js", { attrs: { crossorigin: "anonymous" } }).then(() => (done = true));
  const again = loadScript("/static/demo/once.js");
  await settle();
  expect(tags("once.js").length, "one element for both calls").toEqual(1);
  expect(tags("once.js")[0].getAttribute("crossorigin")).toEqual("anonymous");
  expect(tags("once.js")[0].hasAttribute("async"), "async unless the call says otherwise").toEqual(true);
  expect(done, "not before the browser says it loaded").toEqual(false);
  tags("once.js")[0].dispatchEvent(new Event("load"));
  await first;
  await again;
  expect(done).toEqual(true);
});

test("a script that fails to load rejects with its source named", async () => {
  const loading = loadScript("/static/demo/missing.js");
  await settle();
  tags("missing.js")[0].dispatchEvent(new Event("error"));
  let message = "";
  await loading.catch((e: Error) => (message = e.message));
  expect(message).toEqual("/static/demo/missing.js did not load");
});

test("an idle script waits for the main thread to be idle", async () => {
  const g = globalThis as { requestIdleCallback?: unknown };
  const before = g.requestIdleCallback;
  let idle: (() => void) | null = null;
  g.requestIdleCallback = (cb: () => void) => (idle = cb);
  try {
    void loadScript("/static/demo/idle.js", { when: "idle" });
    await settle();
    expect(tags("idle.js").length, "nothing while the page is busy").toEqual(0);
    idle!();
    await settle();
    expect(tags("idle.js").length).toEqual(1);
  } finally {
    g.requestIdleCallback = before;
  }
});

test("an interaction script waits for the visitor's first key, pointer, touch or scroll", async () => {
  void loadScript("/static/demo/interaction.js", { when: "interaction" });
  await settle();
  expect(tags("interaction.js").length, "nothing until the visitor does something").toEqual(0);
  document.dispatchEvent(new Event("keydown"));
  await settle();
  expect(tags("interaction.js").length).toEqual(1);
});

test("a head row's scripts load once, which is how a streamed resolution's meta gets its script", async () => {
  const row = { "data-sf-script": "load", "data-src": "/static/demo/streamed.js", "data-type": "module" };
  applyHead({ title: "Streamed", scripts: [row] });
  applyHead({ scripts: [row] });
  await settle();
  expect(tags("streamed.js").length).toEqual(1);
  expect(tags("streamed.js")[0].getAttribute("type"), "the row's type comes back from data-type").toEqual("module");
});
