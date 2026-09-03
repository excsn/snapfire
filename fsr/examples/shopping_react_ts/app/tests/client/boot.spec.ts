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
