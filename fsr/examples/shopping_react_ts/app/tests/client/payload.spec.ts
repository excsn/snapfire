import { nodeToHtml, parsePayload, renderSegment } from "@snapfire/fsr-client";
import { expect, test } from "@snapfire/fsr-client/testing";

const wire = ['V {"fmt":1,"enc":"json"}', 'N ["q",[["t","a<b"],["c",{"m":"x#y","p":{"n":{"$":"i","v":"1"}},"s":["r","<p>hi</p>"]}],["p",1,["t","soon"]]]]', 'G {"k":"shell#document","c":[{"k":"x#y","p":[1],"c":[]}]}', 'S 1 ["t","done"]', ""].join("\n");

test("a wire response parses into its tree, its sidecar and its resolutions", () => {
  const payload = parsePayload(wire);
  expect(payload.format).toEqual(1);
  expect(payload.encoding).toEqual("json");
  expect(payload.tree.kind).toEqual("seq");
  const island = payload.tree.kind === "seq" ? payload.tree.children[1] : null;
  expect(island && island.kind === "client").toBeTruthy();
  if (island && island.kind === "client") {
    expect(island.module).toEqual("x#y");
    expect(island.props, "props decode through the value model").toEqual({ n: 1 });
    expect(island.ssr && island.ssr.kind === "raw").toBeTruthy();
  }
  expect(payload.segments).toEqual({ k: "shell#document", c: [{ k: "x#y", p: [1], c: [] }] });
  expect(payload.resolutions).toEqual([{ slot: 1, node: { kind: "text", text: "done" } }]);
});

test("a tree renders to the markers the boot scans for", () => {
  const html = nodeToHtml(parsePayload(wire).tree, { next: 0 });
  expect(html).toEqual(
    'a&lt;b<sf-i id="sf-c0" data-sf-module="x#y"><p>hi</p></sf-i><script type="application/json" data-sf-props="sf-c0">{"n":{"$":"i","v":"1"}}</script><div data-sf-slot="1">soon</div>',
  );
});

test("a segment renders inside its delimiters with the key escaped", () => {
  const html = renderSegment({ kind: "text", text: "x" }, { k: "a-b%c", c: [] }, { next: 0 });
  expect(html).toEqual("<!--sf-g:a%2Db%25c-->x<!--/sf-g-->");
});

test("a row kind the reader does not know is an error", () => {
  expect(() => parsePayload('V {"fmt":1,"enc":"json"}\nX 1')).toThrow("unknown payload row tag");
});
