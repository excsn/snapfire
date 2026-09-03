import { nodeToHtml, parsePayload, renderSegment } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

const wire = ['V {"fmt":1,"enc":"json"}', 'N ["q",[["t","a<b"],["c",{"m":"x#y","p":{"n":{"$":"i","v":"1"}},"s":["r","<p>hi</p>"]}],["p",1,["t","soon"]]]]', 'G {"k":"shell#document","c":[{"k":"x#y","p":[1],"c":[]}]}', 'S 1 ["t","done"]', ""].join("\n");

test("a wire response parses into its tree, its sidecar and its resolutions", () => {
  const payload = parsePayload(wire);
  assert.equal(payload.format, 1);
  assert.equal(payload.encoding, "json");
  assert.equal(payload.tree.kind, "seq");
  const island = payload.tree.kind === "seq" ? payload.tree.children[1] : null;
  assert.ok(island && island.kind === "client");
  if (island && island.kind === "client") {
    assert.equal(island.module, "x#y");
    assert.equal(island.props, { n: 1 }, "props decode through the value model");
    assert.ok(island.ssr && island.ssr.kind === "raw");
  }
  assert.equal(payload.segments, { k: "shell#document", c: [{ k: "x#y", p: [1], c: [] }] });
  assert.equal(payload.resolutions, [{ slot: 1, node: { kind: "text", text: "done" } }]);
});

test("a tree renders to the markers the boot scans for", () => {
  const html = nodeToHtml(parsePayload(wire).tree, { next: 0 });
  assert.equal(
    html,
    'a&lt;b<sf-i id="sf-c0" data-sf-module="x#y"><p>hi</p></sf-i><script type="application/json" data-sf-props="sf-c0">{"n":1}</script><div data-sf-slot="1">soon</div>',
  );
});

test("a segment renders inside its delimiters with the key escaped", () => {
  const html = renderSegment({ kind: "text", text: "x" }, { k: "a-b%c", c: [] }, { next: 0 });
  assert.equal(html, "<!--sf-g:a%2Db%25c-->x<!--/sf-g-->");
});

test("a row kind the reader does not know is an error", () => {
  assert.throws(() => parsePayload('V {"fmt":1,"enc":"json"}\nX 1'), "unknown payload row tag");
});
