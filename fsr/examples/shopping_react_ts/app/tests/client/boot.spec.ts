import { registerIsland, scan } from "@snapfire/fsr-client";
import { assert, settle, test } from "@snapfire/fsr-client/testing";

test("scan mounts each registered marker once, with its props and whether it has markup", async () => {
  const mounted: unknown[] = [];
  registerIsland("tests/client/widget", {
    loader: async () => "widget",
    mount: (mod, props, el, hydrate) => {
      mounted.push([mod, props, el.id, hydrate]);
    },
  });
  document.body.innerHTML =
    '<sf-i id="w1" data-sf-module="tests/client/widget"><b>x</b></sf-i><script type="application/json" data-sf-props="w1">{"n":{"$":"i","v":"2"}}</script><sf-i id="w2" data-sf-module="tests/client/widget"></sf-i>';
  scan(document);
  scan(document);
  await settle();
  assert.equal(mounted, [
    ["widget", { n: 2 }, "w1", true],
    ["widget", {}, "w2", false],
  ]);
});

test("a marker no registry knows yet is not reported until every entry has run", async () => {
  const warnings: string[] = [];
  const warn = console.warn;
  console.warn = (...args: unknown[]) => {
    warnings.push(String(args[0]));
  };
  try {
    document.body.innerHTML = '<sf-i id="l1" data-sf-module="tests/client/late"></sf-i><sf-i id="g1" data-sf-module="tests/client/gone"></sf-i>';
    scan(document);
    assert.equal(warnings, [], "a first miss is what a mounted site's islands look like before its entry runs");
    registerIsland("tests/client/late", { loader: async () => "late", mount: () => {} });
    await settle();
    assert.equal(warnings, ["sf: no island registered for tests/client/gone"], "only the one still missing once the scan settles");
  } finally {
    console.warn = warn;
  }
});
