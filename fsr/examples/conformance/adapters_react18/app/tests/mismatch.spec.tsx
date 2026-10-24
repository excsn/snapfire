import { encodeValue, scan, type SfValue } from "@snapfire/fsr-client";
import { expect, settle, spyOn, test } from "@snapfire/fsr-client/testing";

const placed = (id: string, module: string, html: string): string =>
  `<sf-s data-sf-island data-sf-when="load"><sf-i id="${id}" data-sf-module="${module}" data-sf-ssr>${html}</sf-i><script type="application/json" data-sf-props="${id}">${JSON.stringify(encodeValue({ label: id, nest: [] } as SfValue))}</script></sf-s>`;

const reportedFor = async (module: string): Promise<string[]> => {
  const reported = spyOn(console, "error").mockImplementation(() => {});
  const warned = spyOn(console, "warn").mockImplementation(() => {});
  document.body.innerHTML = placed("first", module, "<p>not what it renders</p>") + placed("second", module, "<p>nor this</p>");
  scan(document);
  await settle();
  const messages = reported.mock.calls.map((call) => String(call[0]));
  reported.mockRestore();
  warned.mockRestore();
  return messages.filter((message) => message.includes("did not hydrate"));
};

test("each Vue island that does not hydrate to the server's markup is reported as an error", async () => {
  const reported = await reportedFor("src/ui/VueProbe.vue#default");
  expect(reported.length).toEqual(2);
  expect(reported[0]).toContain("src/ui/VueProbe.vue#default");
});

test("each Svelte island that does not hydrate to the server's markup is reported as an error", async () => {
  const reported = await reportedFor("src/ui/SvelteProbe.svelte#default");
  expect(reported.length).toEqual(2);
  expect(reported[0]).toContain("src/ui/SvelteProbe.svelte#default");
});

test("each React island that does not hydrate to the server's markup is reported as an error", async () => {
  const reported = await reportedFor("src/ui/ReactProbe.tsx#default");
  expect(reported.length).toEqual(2);
  expect(reported[0]).toContain("src/ui/ReactProbe.tsx#default");
});
