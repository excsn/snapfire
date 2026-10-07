import { clear, set } from "@snapfire/fsr-client/store";
import { ctx, describe, expect, fireEvent, load, render, settle, spyOn, test, waitFor } from "@snapfire/fsr-client/testing";

import { tally } from "@src/probes";
import { probeCount, probeOther } from "@src/store";
import HeldProbe from "@src/ui/HeldProbe";
import HeldVueProbe from "@src/ui/HeldVueProbe.vue";
import ReactProbe from "@src/ui/ReactProbe";
import VueProbe from "@src/ui/VueProbe.vue";

const REACT = "src/ui/ReactProbe.tsx#default";
const VUE = "src/ui/VueProbe.vue#default";

const FRAMEWORKS = [
  { owner: "react", module: REACT, other: VUE, Probe: ReactProbe, Held: HeldProbe },
  { owner: "vue", module: VUE, other: REACT, Probe: VueProbe, Held: HeldVueProbe },
] as const;

const page = async (path = "/", locale?: string) => {
  await load(path, { ctx: ctx(locale === undefined ? {} : { locale }) });
  await settle();
};

const probe = (owner: string) => document.querySelector<HTMLElement>(`.probes > sf-s > sf-i > [data-owner="${owner}"]`)!;

const counts = () => [...document.querySelectorAll(".probe .count")].map((count) => count.textContent);

/** The props the server writes on an island it rendered from `values`, the store keys it held. */
const renderedFrom = (values: Record<string, unknown>) => ({ $sv: values }) as object;

/** What the frameworks logged about hydration while `body` ran. */
async function hydrationWarnings(body: () => Promise<unknown>): Promise<string[]> {
  const warned = spyOn(console, "warn");
  const errored = spyOn(console, "error");
  try {
    await body();
    return [...warned.mock.calls, ...errored.mock.calls].map((args) => args.map(String).join(" ")).filter((line) => /hydrat/i.test(line));
  } finally {
    warned.mockRestore();
    errored.mockRestore();
  }
}

describe.each(FRAMEWORKS)("the $owner adapter", ({ owner, module, other, Probe, Held }) => {
  test("mounts the island and hydrates over the server's markup", async () => {
    const before = tally().renders[owner] ?? 0;
    const r = await render(<Probe label="alone" nest={[]} />);
    expect(r.hydrated, "the server rendered it, so the adapter hydrated").toEqual(module);
    expect(r.container.querySelector("sf-i")?.hasAttribute("data-sf-mounted")).toEqual(true);
    expect(r.getByText("alone")).toBeTruthy();
    expect(tally().renders[owner], "the framework ran it").toBeGreaterThan(before);
  });

  test("patches the island in place", async () => {
    const r = await render(<Probe label="first" nest={[]} />);
    const button = r.getByText("add");
    await r.rerender(<Probe label="second" nest={[]} />);
    expect(r.getByText("second")).toBeTruthy();
    expect(r.queryByText("first")).toBeNull();
    expect(r.getByText("add"), "the patch kept the element").toBe(button);
  });

  test("unmounts the island", async () => {
    const r = await render(<Probe label="gone" nest={[]} />);
    const before = tally().unmounts[owner] ?? 0;
    r.unmount();
    await settle();
    expect(tally().unmounts[owner], "the framework ran its cleanup").toEqual(before + 1);
    expect(r.queryByText("gone")).toBeNull();
  });

  test("renders the children the page placed inside it", async () => {
    await page();
    const children = probe(owner).querySelector(".children em");
    expect(children?.textContent).toEqual(`${owner} children`);
  });

  test("places an island of the other framework", async () => {
    await page();
    const nested = probe(owner).querySelector(`sf-i[data-sf-module="${other}"]`);
    expect(nested?.hasAttribute("data-sf-mounted"), "the nested island mounted through its own adapter").toEqual(true);
    expect(nested?.querySelector(".label")?.textContent).toEqual(`${owner} nested`);
  });

  test("binds the store, so a write shows in every island", async () => {
    await page();
    expect(counts().every((count) => count === "0"), counts().join()).toEqual(true);
    await fireEvent.click(probe(owner).querySelector(".add")!);
    await settle();
    expect(counts().length, "both probes, both nested probes and both server islands").toEqual(6);
    expect(counts().every((count) => count === "1"), counts().join()).toEqual(true);
  });

  test("hydrates over a key another island wrote while the server held none", async () => {
    set(probeCount, 5);
    let r: Awaited<ReturnType<typeof render>> | undefined;
    const warnings = await hydrationWarnings(async () => {
      r = await render(<Probe label="late" nest={[]} />);
    });
    clear(probeCount);
    expect(warnings, "the first render matches the server's, which rendered `initial`").toEqual([]);
    expect(r!.container.querySelector(".count")?.textContent, "then it moves to the store's value").toEqual("5");
  });

  test("hydrates a key the server held none of from `initial` beside one it held", async () => {
    set(probeCount, 7);
    set(probeOther, 5);
    let r: Awaited<ReturnType<typeof render>> | undefined;
    const warnings = await hydrationWarnings(async () => {
      r = await render(<Held label="held" {...renderedFrom({ "probe/count": 0 })} />);
    });
    clear(probeCount);
    clear(probeOther);
    expect(warnings, "the server rendered `probe/count` as held and `probe/other` from `initial`").toEqual([]);
    expect([".held", ".other"].map((at) => r!.container.querySelector(at)?.textContent), "then both move to the store's values").toEqual(["7", "5"]);
  });

  for (const [server, values] of [["held the key", { "probe/count": 0 }], ["held nothing", null]] as const) {
    test(`renders a component created after hydration from the store on its first render when the server ${server}`, async () => {
      const r = await render(<Held label="late" {...(values === null ? {} : renderedFrom(values))} />);
      set(probeCount, 5);
      await settle();
      const before = tally().renders[`${owner}-late`] ?? 0;
      await fireEvent.click(r.container.querySelector(".more")!);
      await settle();
      const renders = (tally().renders[`${owner}-late`] ?? 0) - before;
      clear(probeCount);
      expect(r.container.querySelector(".late")?.textContent).toEqual("5");
      expect(renders, "one render, of the store's value, with no hydration to agree with").toEqual(1);
    });
  }

  test("binds the locale the request chose", async () => {
    await page();
    expect(probe(owner).querySelector(".locale")?.textContent).toEqual("en");
    await page("/fr/");
    expect(probe(owner).querySelector(".locale")?.textContent).toEqual("fr");
    await page("/", "fr");
    expect(probe(owner).querySelector(".locale")?.textContent, "the ctx is the request the page loads under").toEqual("fr");
  });

  test("writes a Link the navigator reads", async () => {
    await page();
    const link = probe(owner).querySelector("a.next");
    expect(link?.getAttribute("href")).toEqual("/next");
    expect(link?.hasAttribute("data-sf-link"), link?.outerHTML).toEqual(true);
  });

  test("writes a Picture", async () => {
    await page();
    const img = probe(owner).querySelector("img");
    expect(img?.getAttribute("src")).toEqual("/pictures/probe.png");
    expect(img?.getAttribute("alt")).toEqual("probe");
    expect(img?.getAttribute("width")).toEqual("4");
    expect(img?.getAttribute("height")).toEqual("4");
  });

  test("is discarded when a navigation replaces its page", async () => {
    await page();
    const before = tally().unmounts[owner] ?? 0;
    await fireEvent.click(probe(owner).querySelector("a.next")!);
    await settle();
    expect(location.pathname).toEqual("/next");
    expect(document.querySelector(".next-page")).toBeTruthy();
    expect(document.querySelector(".probe"), "no probe is left").toBeNull();
    expect(tally().unmounts[owner], "the island and the one nested in the other probe").toEqual(before + 2);
  });
});

const SERVER = [
  { owner: "server", written: "react" },
  { owner: "server-vue", written: "vue" },
] as const;

describe.each(SERVER)("the $owner island", ({ owner, written }) => {
  test("renders the store's value and follows a write from another island", async () => {
    await page();
    expect(probe(owner).querySelector(".count")?.textContent).toEqual("0");
    await fireEvent.click(probe(written).querySelector(".add")!);
    await settle();
    await waitFor(() => expect(probe(owner).querySelector(".count")?.textContent, "the write reached the server island, which stepped again").toEqual("1"));
  });

  test("writes the store, so every island shows its write", async () => {
    await page();
    await fireEvent.click(probe(owner).querySelector(".add")!);
    await settle();
    await waitFor(() => expect(counts().every((count) => count === "1"), counts().join()).toEqual(true));
    expect(counts().length).toEqual(6);
  });
});

describe("a custom element", () => {
  test("is defined at its timing and upgrades the server's markup", async () => {
    await page();
    expect(customElements.get("probe-element"), "the define module ran").toBeTruthy();
    const element = document.querySelector("probe-element");
    expect(element?.hasAttribute("data-upgraded")).toEqual(true);
    expect(element?.getAttribute("label")).toEqual("element");
  });

  test("keeps the children the page placed inside it", async () => {
    await page();
    expect(document.querySelector("probe-element em")?.textContent).toEqual("element children");
  });

  test("is discarded when a navigation replaces its page", async () => {
    await page();
    const before = tally().unmounts.element ?? 0;
    await fireEvent.click(probe("react").querySelector("a.next")!);
    await settle();
    expect(document.querySelector("probe-element")).toBeNull();
    expect(tally().unmounts.element).toEqual(before + 1);
  });
});
