import { ctx, describe, expect, fireEvent, load, render, settle, test } from "@snapfire/fsr-client/testing";

import { tally } from "@src/probes";
import ReactProbe from "@src/ui/ReactProbe";
import VueProbe from "@src/ui/VueProbe.vue";

const REACT = "src/ui/ReactProbe.tsx#default";
const VUE = "src/ui/VueProbe.vue#default";

const FRAMEWORKS = [
  { owner: "react", module: REACT, other: VUE, Probe: ReactProbe },
  { owner: "vue", module: VUE, other: REACT, Probe: VueProbe },
] as const;

const page = async (path = "/") => {
  await load(path, { ctx: ctx() });
  await settle();
};

const probe = (owner: string) => document.querySelector<HTMLElement>(`.probes > sf-s > sf-i > [data-owner="${owner}"]`)!;

const counts = () => [...document.querySelectorAll(".probe .count")].map((count) => count.textContent);

describe.each(FRAMEWORKS)("the $owner adapter", ({ owner, module, other, Probe }) => {
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
    expect(counts().length, "both probes and both nested probes").toEqual(4);
    expect(counts().every((count) => count === "1"), counts().join()).toEqual(true);
  });

  test("binds the locale the request chose", async () => {
    await page();
    expect(probe(owner).querySelector(".locale")?.textContent).toEqual("en");
    await page("/fr/");
    expect(probe(owner).querySelector(".locale")?.textContent).toEqual("fr");
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
