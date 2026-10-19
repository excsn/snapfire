import { clear, set } from "@snapfire/fsr-client/store";
import { ctx, describe, expect, fireEvent, load, render, settle, spyOn, test, waitFor } from "@snapfire/fsr-client/testing";

import { tally } from "@src/probes";
import { probeCount, probeOther } from "@src/store";
import HeldProbe from "@src/ui/HeldProbe";
import HeldVueProbe from "@src/ui/HeldVueProbe.vue";
import LazyProbe from "@src/ui/LazyProbe";
import LazyVueProbe from "@src/ui/LazyVueProbe.vue";
import ReactProbe from "@src/ui/ReactProbe";
import SvelteProbe from "@src/ui/SvelteProbe.svelte";
import VueProbe from "@src/ui/VueProbe.vue";

const REACT = "src/ui/ReactProbe.tsx#default";
const VUE = "src/ui/VueProbe.vue#default";

const FRAMEWORKS = [
  { owner: "react", module: REACT, other: VUE, Probe: ReactProbe, Held: HeldProbe, Lazy: LazyProbe, placed: 3 },
  { owner: "vue", module: VUE, other: REACT, Probe: VueProbe, Held: HeldVueProbe, Lazy: LazyVueProbe, placed: 2 },
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

describe.each(FRAMEWORKS)("the $owner adapter", ({ owner, module, other, Probe, Held, Lazy, placed }) => {
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
    expect(counts().length, "both probes, the svelte probe, the three nested probes and both server islands").toEqual(8);
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

  test("hydrates a lazily loaded child against the values the server rendered it from", async () => {
    set(probeCount, 7);
    let r: Awaited<ReturnType<typeof render>> | undefined;
    const warnings = await hydrationWarnings(async () => {
      r = await render(<Lazy label="lazy" {...renderedFrom({ "probe/count": 0 })} />);
      await waitFor(() => expect(r!.container.querySelector(".lazy-part")).toBeTruthy());
      await settle();
    });
    clear(probeCount);
    expect(warnings, "the server rendered the lazy part from the held value and the browser hydrated it against the same").toEqual([]);
    expect([".held", ".lazy-part"].map((at) => r!.container.querySelector(at)?.textContent), "then both move to the store's value").toEqual(["7", "7"]);
  });

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
    expect(tally().unmounts[owner], "the island and every one another probe placed").toEqual(before + placed);
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
    expect(counts().length).toEqual(8);
  });
});

describe("the svelte adapter", () => {
  test("mounts the island in the browser with its props", async () => {
    await page();
    expect(probe("svelte")?.querySelector(".label")?.textContent).toEqual("svelte");
    expect(tally().renders.svelte, "Svelte ran it").toBeGreaterThan(0);
  });

  test("renders the children the page placed inside it", async () => {
    await page();
    expect(probe("svelte").querySelector(".children em")?.textContent).toEqual("svelte children");
  });

  test("binds the store, so its write shows in every island and theirs in it", async () => {
    await page();
    await fireEvent.click(probe("svelte").querySelector(".add")!);
    await settle();
    await waitFor(() => expect(counts().every((count) => count === "1"), counts().join()).toEqual(true));
    await fireEvent.click(probe("react").querySelector(".add")!);
    await settle();
    await waitFor(() => expect(probe("svelte").querySelector(".count")?.textContent).toEqual("2"));
  });

  test("places an island of another framework through the island attachment", async () => {
    await page();
    const nested = probe("svelte").querySelector(`sf-i[data-sf-module="${REACT}"]`);
    expect(nested?.hasAttribute("data-sf-mounted"), "the nested island mounted through its own adapter").toEqual(true);
    expect(nested?.querySelector(".label")?.textContent).toEqual("svelte nested");
  });

  test("patches the nested island when its props change rather than mounting it again", async () => {
    const r = await render(<SvelteProbe label="first" nest={[REACT]} />);
    const nested = r.container.querySelector(`sf-i[data-sf-module="${REACT}"]`);
    await r.rerender(<SvelteProbe label="second" nest={[REACT]} />);
    await settle();
    expect(r.container.querySelector(`sf-i[data-sf-module="${REACT}"]`), "the same marker").toBe(nested);
    expect(nested?.querySelector(".label")?.textContent).toEqual("second nested");
  });

  test("binds the locale the request chose", async () => {
    await page();
    expect(probe("svelte").querySelector(".locale")?.textContent).toEqual("en");
    await page("/fr/");
    expect(probe("svelte").querySelector(".locale")?.textContent).toEqual("fr");
  });

  test("writes a link the navigator reads", async () => {
    await page();
    const link = probe("svelte").querySelector("a.next");
    expect(link?.getAttribute("href")).toEqual("/next");
    expect(link?.hasAttribute("data-sf-link"), link?.outerHTML).toEqual(true);
  });

  test("writes a picture", async () => {
    await page();
    const img = probe("svelte").querySelector("img");
    expect(img?.getAttribute("src")).toEqual("/pictures/probe.png");
    expect(img?.getAttribute("alt")).toEqual("probe");
    expect(img?.getAttribute("width")).toEqual("4");
  });

  test("is discarded with the island it placed when a navigation replaces its page", async () => {
    await page();
    const before = tally().unmounts.svelte ?? 0;
    const reactBefore = tally().unmounts.react ?? 0;
    await fireEvent.click(probe("svelte").querySelector("a.next")!);
    await settle();
    expect(location.pathname).toEqual("/next");
    expect(document.querySelector(".probe"), "no probe is left").toBeNull();
    expect(tally().unmounts.svelte).toEqual(before + 1);
    expect(tally().unmounts.react, "the React probe, the one nested in the Vue probe and the one the svelte probe placed").toEqual(reactBefore + 3);
  });

  test("patches the island in place", async () => {
    const r = await render(<SvelteProbe label="first" />);
    const button = r.getByText("add");
    await r.rerender(<SvelteProbe label="second" />);
    expect(r.getByText("second")).toBeTruthy();
    expect(r.queryByText("first")).toBeNull();
    expect(r.getByText("add"), "the patch kept the element").toBe(button);
  });

  test("unmounts the island", async () => {
    const r = await render(<SvelteProbe label="gone" />);
    const before = tally().unmounts.svelte ?? 0;
    r.unmount();
    await settle();
    expect(tally().unmounts.svelte, "Svelte ran its cleanup").toEqual(before + 1);
    expect(r.queryByText("gone")).toBeNull();
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
