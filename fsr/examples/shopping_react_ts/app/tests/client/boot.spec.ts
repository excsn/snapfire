import { registerDeferredIsland, registerIsland, scan, type Adapter } from "@snapfire/fsr/client";
import { expect, settle, test } from "@snapfire/fsr/client/testing";

test("scan mounts each registered marker once, with its props and whether it has markup", async () => {
  const mounted: unknown[] = [];
  registerIsland("tests/client/widget", {
    loader: async () => "widget",
    mount: (mod, props, el, hydrate) => {
      mounted.push([mod, props, el.id, hydrate]);
    },
  });
  document.body.innerHTML =
    '<sf-i id="w1" data-sf-module="tests/client/widget" data-sf-ssr><b>x</b></sf-i><script type="application/json" data-sf-props="w1">{"n":{"$":"i","v":"2"}}</script><sf-i id="w2" data-sf-module="tests/client/widget"></sf-i>';
  scan(document);
  scan(document);
  await settle();
  expect(mounted).toEqual([
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
    expect(warnings, "a first miss is what a mounted site's islands look like before its entry runs").toEqual([]);
    registerIsland("tests/client/late", { loader: async () => "late", mount: () => {} });
    await settle();
    expect(warnings, "only the one still missing once the scan settles").toEqual(["sf: no island registered for tests/client/gone"]);
  } finally {
    console.warn = warn;
  }
});

test("a deferred island's adapter is imported once a scan finds one of its islands and once for all of them", async () => {
  let imports = 0;
  const mounted: string[] = [];
  const adapter = async (): Promise<Adapter> => {
    imports += 1;
    return { mount: (mod, _props, el) => void mounted.push(`${mod} ${el.id}`) };
  };
  registerDeferredIsland("tests/client/chart", { loader: async () => "chart", adapter });
  registerDeferredIsland("tests/client/table", { loader: async () => "table", adapter });
  document.body.innerHTML = "<p>no island here</p>";
  scan(document);
  await settle();
  expect(imports, "a page holding none of its islands loads no adapter").toEqual(0);
  document.body.innerHTML = '<sf-i id="c1" data-sf-module="tests/client/chart"></sf-i><sf-i id="t1" data-sf-module="tests/client/table"></sf-i>';
  scan(document);
  scan(document);
  await settle();
  expect(imports).toEqual(1);
  expect(mounted).toEqual(["chart c1", "table t1"]);
});
