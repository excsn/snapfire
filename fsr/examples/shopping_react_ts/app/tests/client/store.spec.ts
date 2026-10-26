import { clear, derive, get, key, optimistic, seed, set, snapshot, subscribe, transaction } from "@snapfire/fsr/client";
import { adopt, contribute, restore, retain, take } from "@snapfire/fsr/client/store";
import { expect, test } from "@snapfire/fsr/client/testing";

let n = 0;
/** A fresh key per test, since the store is one map for the whole run. */
function fresh<T>(): ReturnType<typeof key<T>> {
  n += 1;
  return key<T>(`spec/${n}`);
}

test("a key is the string it names, and get reads what set wrote", () => {
  const k = fresh<number>();
  expect(typeof k).toEqual("string");
  expect(get(k)).toBeUndefined();
  set(k, 3);
  expect(get(k)).toEqual(3);
  expect(snapshot()[k]).toEqual(3);
  clear(k);
  expect(get(k)).toBeUndefined();
  expect(k in snapshot()).toEqual(false);
});

test("a listener hears every change, never the value already held, and stops when unsubscribed", () => {
  const k = fresh<number>();
  const seen: unknown[] = [];
  const stop = subscribe(k, (value, name) => seen.push([name, value]));
  set(k, 1);
  set(k, 1);
  set(k, 2);
  clear(k);
  clear(k);
  expect(seen).toEqual([
    [k, 1],
    [k, 2],
    [k, undefined],
  ]);
  stop();
  set(k, 9);
  expect(seen.length).toEqual(3);
});

test("a listener added or removed during a notification takes effect from the next one", () => {
  const k = fresh<number>();
  const seen: string[] = [];
  const late = () => seen.push("late");
  const once = () => {
    seen.push("once");
    stopOnce();
    subscribe(k, late);
  };
  const stopOnce = subscribe(k, once);
  set(k, 1);
  expect(seen, "the listener added mid-notification did not hear this one").toEqual(["once"]);
  set(k, 2);
  expect(seen, "the one-shot listener is gone and the late one hears").toEqual(["once", "late"]);
});

test("a transaction collapses notifications to one per key, and a nested one defers to the outermost", () => {
  const a = fresh<number>();
  const b = fresh<number>();
  const seen: string[] = [];
  subscribe(a, () => seen.push("a"));
  subscribe(b, () => seen.push("b"));
  transaction(() => {
    set(a, 1);
    set(a, 2);
    transaction(() => {
      set(b, 1);
      set(a, 3);
    });
    expect(seen, "nothing fires before the outermost block ends").toEqual([]);
    expect(get(a), "reads inside see the writes").toEqual(3);
  });
  expect(seen).toEqual(["a", "b"]);
  expect(get(a)).toEqual(3);
});

test("a transaction that throws still fires what it dirtied and leaves the store out of it", () => {
  const k = fresh<number>();
  const seen: unknown[] = [];
  subscribe(k, (v) => seen.push(v));
  expect(() =>
    transaction(() => {
      set(k, 5);
      throw new Error("halfway");
    }),
  ).toThrow();
  expect(seen).toEqual([5]);
  set(k, 6);
  expect(seen, "later writes notify at once again").toEqual([5, 6]);
});

test("a derived key computes now and recomputes when a source changes, once per change", () => {
  const a = fresh<number>();
  const b = fresh<number>();
  const sum = fresh<number>();
  set(a, 1);
  set(b, 2);
  const seen: unknown[] = [];
  subscribe(sum, (v) => seen.push(v));
  derive(sum, [a, b], (read) => (read(a) ?? 0) + (read(b) ?? 0));
  expect(get(sum), "computed at registration").toEqual(3);
  expect(seen).toEqual([3]);
  set(a, 10);
  expect(get(sum)).toEqual(12);
  set(b, 2);
  expect(seen, "a source written with its own value changes nothing").toEqual([3, 12]);
  transaction(() => {
    set(a, 0);
    set(b, 0);
  });
  expect(get(sum)).toEqual(0);
  expect(seen, "two source writes in one transaction recompute once").toEqual([3, 12, 0]);
});

test("a derived key feeds another derived key", () => {
  const a = fresh<number>();
  const twice = fresh<number>();
  const label = fresh<string>();
  set(a, 2);
  derive(twice, [a], (read) => (read(a) ?? 0) * 2);
  derive(label, [twice], (read) => `x${read(twice)}`);
  expect(get(label)).toEqual("x4");
  set(a, 5);
  expect(get(label)).toEqual("x10");
});

test("optimistic shows the guess, keeps it on success and puts the key back on failure", async () => {
  const k = fresh<number>();
  set(k, 1);
  const seen: unknown[] = [];
  subscribe(k, (v) => seen.push(v));
  const result = await optimistic(k, 2, async () => "ok");
  expect(result).toEqual("ok");
  expect(get(k), "a success leaves the guess for the revalidation to replace").toEqual(2);
  await expect(optimistic(k, 3, async () => Promise.reject(new Error("no")))).rejects.toThrow();
  expect(get(k), "a failure restores what the key held").toEqual(2);
  expect(seen).toEqual([2, 3, 2]);
});

test("optimistic on a key nothing set clears it again on failure", async () => {
  const k = fresh<number>();
  await expect(optimistic(k, 7, async () => Promise.reject(new Error("no")))).rejects.toThrow();
  expect(get(k)).toBeUndefined();
  expect(k in snapshot()).toEqual(false);
});

test("seed writes a whole map in one transaction and the server's value wins a local one", () => {
  const a = fresh<number>();
  const b = fresh<string>();
  const seen: string[] = [];
  subscribe(a, () => seen.push("a"));
  subscribe(b, () => seen.push("b"));
  set(a, 99);
  seed({ [a]: 1, [b]: "two" });
  expect(get(a)).toEqual(1);
  expect(get(b)).toEqual("two");
  expect(seen).toEqual(["a", "a", "b"]);
  seed({ [a]: 1 });
  expect(seen, "a seed equal to what is held notifies nobody").toEqual(["a", "a", "b"]);
});

test("a default seeds a key the browser holds nothing for and yields to a write when the segment seeds again", () => {
  const qty = fresh<number>();
  const name = fresh<string>();
  contribute([{ k: "spec/order", p: [], v: { [name]: "Kiambu" }, d: { [qty]: 1 } }]);
  expect(get(qty)).toEqual(1);
  expect(get(name)).toEqual("Kiambu");
  set(qty, 3);
  set(name, "Nyeri");
  contribute([{ k: "spec/order", p: [], v: { [name]: "Kiambu" }, d: { [qty]: 1 } }]);
  expect(get(qty)).toEqual(3);
  expect(get(name)).toEqual("Kiambu");
  contribute([{ k: "spec/order", p: [], v: { [name]: "Kiambu", [qty]: 2 } }]);
  expect(get(qty)).toEqual(2);
});

/** A segment region holding what a fragment swap left: its markup and its seed script. */
function region(segment: string): Element {
  const host = document.createElement("div");
  host.append(document.createComment(`sf-g:${segment}`), document.createComment("/sf-g"));
  document.body.append(host);
  return host;
}

function swapIn(host: Element, seed: unknown): Element {
  const script = document.createElement("script");
  script.type = "application/json";
  script.setAttribute("data-sf-store", "");
  script.setAttribute("data-sf-fragment", "");
  script.textContent = JSON.stringify(seed);
  host.insertBefore(script, host.lastChild);
  adopt();
  return script;
}

test("a fragment's seed for a segment the document does not show stays while the region it landed in does", () => {
  const [a, b] = [fresh<number>(), fresh<number>()];
  const host = region("spec#owner");
  swapIn(host, [{ k: "spec#fragment?n=1", p: ["content"], v: { [a]: 1, [b]: 1 } }]);
  expect([get(a), get(b)]).toEqual([1, 1]);
  retain(["spec#owner"]);
  expect([get(a), get(b)], "the region's segment is still shown").toEqual([1, 1]);
  retain(["spec#elsewhere"]);
  expect([get(a), get(b)], "it left and the seed went with it").toEqual([undefined, undefined]);
  host.remove();
});

test("the next fragment of the route swapped into the same region replaces the seed the last one carried", () => {
  const [a, b] = [fresh<number>(), fresh<number>()];
  const host = region("spec#owner");
  const first = swapIn(host, [{ k: "spec#fragment?n=1", p: ["content"], v: { [a]: 1, [b]: 1 } }]);
  first.remove();
  swapIn(host, [{ k: "spec#fragment?n=2", p: ["content"], v: { [a]: 2 } }]);
  expect(get(a)).toEqual(2);
  expect(get(b), "the last fragment's seed is gone, not merged under the new one").toBeUndefined();
  retain([]);
  host.remove();
});

test("a fragment's seed for a segment the document shows is that segment's", () => {
  const a = fresh<number>();
  const host = region("spec#owner");
  const shown = region("spec#layout");
  swapIn(host, [{ k: "spec#layout", p: [], v: { [a]: 5 } }]);
  retain(["spec#layout"]);
  expect(get(a), "kept by the layout, which the document shows").toEqual(5);
  retain([]);
  host.remove();
  shown.remove();
});

test("take hands back a held segment's seeds and those of what its region holds while restore puts back only what nothing seeded since", () => {
  const [a, b] = [fresh<number>(), fresh<number>()];
  const host = region("spec#held");
  contribute([{ k: "spec#held", p: ["content"], v: { [a]: 1 } }]);
  swapIn(host, [{ k: "spec#fragment", p: ["content", "content"], v: { [b]: 1 } }]);
  const seeds = take(["spec#held"]);
  expect(seeds.length).toEqual(2);
  expect([get(a), get(b)], "out of the store while held").toEqual([undefined, undefined]);
  contribute([{ k: "spec#held", p: ["content"], v: { [a]: 7 } }]);
  restore(seeds);
  expect(get(a), "the payload that brought the page back seeded it again and wins").toEqual(7);
  expect(get(b), "the fragment's seed came back with the page").toEqual(1);
  retain([]);
  host.remove();
});
