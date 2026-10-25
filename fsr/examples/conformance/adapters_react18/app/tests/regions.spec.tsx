import { scan } from "@snapfire/fsr/client";
import { expect, fireEvent, settle, spyOn, test } from "@snapfire/fsr/client/testing";

const FRAMES = [
  ["React", "src/ui/Frame.tsx#default"],
  ["Vue", "src/ui/FrameVue.vue#default"],
  ["Svelte", "src/ui/FrameSvelte.svelte#default"],
] as const;

const placed = (id: string, module: string, body: string, rendered: boolean): string =>
  `<sf-s data-sf-island data-sf-when="load"><sf-i id="${id}" data-sf-module="${module}"${rendered ? " data-sf-ssr" : ""}>${body}</sf-i><script type="application/json" data-sf-props="${id}">{}</script></sf-s>`;

const children = (html: string): string => `<sf-s data-sf-children>${html}</sf-s>`;

/** Boots `html` and reports whether the element matching `selector` is still the node the server wrote, with a listener bound to it before the boot still answering. */
const keeps = async (html: string, selector: string): Promise<{ mounted: boolean; same: boolean; clicks: number }> => {
  document.body.innerHTML = html;
  const bound = document.querySelector(selector) as Element;
  let clicks = 0;
  bound.addEventListener("click", () => clicks++);
  scan(document);
  await settle();
  const now = document.querySelector(selector);
  if (now) await fireEvent.click(now);
  return { mounted: document.querySelector(".frame .tick") !== null, same: now === bound, clicks };
};

for (const [framework, module] of FRAMES) {
  test(`a ${framework} island the browser mounts keeps the nodes the server wrote among its children`, async () => {
    const kept = await keeps(placed("m1", module, children('<p class="bound">server</p>'), false), ".bound");
    expect(kept.mounted, "the island mounted around them").toEqual(true);
    expect(kept.same, "the paragraph is the server's node, not a copy").toEqual(true);
    expect(kept.clicks, "what was bound to it before the mount still answers").toEqual(1);
  });

  test(`a ${framework} island whose markup does not hydrate keeps the nodes the server wrote among its children`, async () => {
    const reported = spyOn(console, "error").mockImplementation(() => {});
    const warned = spyOn(console, "warn").mockImplementation(() => {});
    const kept = await keeps(placed("h1", module, `<section class="frame"><b>not what it renders</b>${children('<p class="bound">server</p>')}</section>`, true), ".bound");
    reported.mockRestore();
    warned.mockRestore();
    expect(kept.mounted, "the island rendered").toEqual(true);
    expect(kept.same, "the paragraph is the server's node, not a copy").toEqual(true);
    expect(kept.clicks).toEqual(1);
  });
}

for (const [framework, module] of FRAMES) {
  test(`a ${framework} island the browser mounts keeps the nodes inside an element beside a ${framework} island among its children`, async () => {
    const nested = `<sf-s data-sf-island><sf-i id="n1" data-sf-module="${module}"></sf-i><script type="application/json" data-sf-props="n1">{}</script></sf-s>`;
    const kept = await keeps(placed("m2", module, children(`<div class="row"><button class="inner">server</button></div>${nested}`), false), ".inner");
    expect(document.querySelectorAll(".frame").length, "both islands rendered").toEqual(2);
    expect(kept.same, "the button is the server's node, not a copy").toEqual(true);
    expect(kept.clicks).toEqual(1);
  });
}
