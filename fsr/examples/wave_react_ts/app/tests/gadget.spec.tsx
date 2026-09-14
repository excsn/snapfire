import { morph, registerIsland, scan } from "@snapfire/fsr-client";
import { expect, settle, test } from "@snapfire/fsr-client/testing";

/// A gadget is a server island, so every click patches its markup in with `morph`. An island placed inside it is not the gadget's to redraw: its marker keeps the id and the marks the document gave it, its children stay as the browser drew them and only its props follow the gadget's render.
test("a gadget's step leaves an island inside it alone and hands it the new props", async () => {
  document.body.innerHTML =
    '<sf-s data-sf-island data-sf-mode="server"><sf-i id="sf-i3" data-sf-module="spec/Gadget"><p>turn x</p>' +
    '<sf-s data-sf-island data-sf-mode="browser"><sf-i id="sf-i4" data-sf-module="spec/Chart">drawn</sf-i><script type="application/json" data-sf-props="sf-i4">{"n":1}</script></sf-s>' +
    '</sf-i><script type="application/json" data-sf-props="sf-i3">{"$s":{}}</script></sf-s>';
  const patched: unknown[] = [];
  registerIsland("spec/Chart", {
    loader: async () => ({}),
    mount: (_module, _props, el) => {
      el.textContent = "drawn by the browser";
      return {};
    },
    patch: (_handle, _module, props) => {
      patched.push(props);
    },
  });
  scan(document);
  await settle();
  const gadget = document.getElementById("sf-i3") as Element;
  morph(
    gadget,
    '<p>turn o</p><sf-s data-sf-island data-sf-mode="browser"><sf-i id="sf-i0" data-sf-module="spec/Chart"></sf-i><script type="application/json" data-sf-props="sf-i0">{"n":2}</script></sf-s>',
  );
  await settle();
  expect(gadget.querySelector("p")?.textContent, "the gadget's own markup is patched").toEqual("turn o");
  const chart = gadget.querySelector("sf-i") as Element;
  expect(chart.id, "the island keeps the id the document gave it rather than the step's").toEqual("sf-i4");
  expect(chart.textContent, "and what the browser drew").toEqual("drawn by the browser");
  expect(chart.hasAttribute("data-sf-mounted"), "and the mark that says it mounted").toBeTruthy();
  expect(chart.nextElementSibling?.getAttribute("data-sf-props"), "its props script still names it").toEqual("sf-i4");
  expect(chart.nextElementSibling?.textContent, "and carries the step's props").toEqual('{"n":2}');
  expect(patched, "which reached the island").toEqual([{ n: 2 }]);
});
