import { clear, derive, get, key, optimistic, seed, set, snapshot, subscribe, transaction } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

let n = 0;
/** A fresh key per test, since the store is one map for the whole run. */
function fresh<T>(): ReturnType<typeof key<T>> {
  n += 1;
  return key<T>(`spec/${n}`);
}

test("a key is the string it names, and get reads what set wrote", () => {
  const k = fresh<number>();
  assert.equal(typeof k, "string");
  assert.equal(get(k), undefined);
  set(k, 3);
  assert.equal(get(k), 3);
  assert.equal(snapshot()[k], 3);
  clear(k);
  assert.equal(get(k), undefined);
  assert.equal(k in snapshot(), false);
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
  assert.equal(seen, [
    [k, 1],
    [k, 2],
    [k, undefined],
  ]);
  stop();
  set(k, 9);
  assert.equal(seen.length, 3);
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
  assert.equal(seen, ["once"], "the listener added mid-notification did not hear this one");
  set(k, 2);
  assert.equal(seen, ["once", "late"], "the one-shot listener is gone and the late one hears");
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
    assert.equal(seen, [], "nothing fires before the outermost block ends");
    assert.equal(get(a), 3, "reads inside see the writes");
  });
  assert.equal(seen, ["a", "b"]);
  assert.equal(get(a), 3);
});

test("a transaction that throws still fires what it dirtied and leaves the store out of it", () => {
  const k = fresh<number>();
  const seen: unknown[] = [];
  subscribe(k, (v) => seen.push(v));
  assert.throws(() =>
    transaction(() => {
      set(k, 5);
      throw new Error("halfway");
    }),
  );
  assert.equal(seen, [5]);
  set(k, 6);
  assert.equal(seen, [5, 6], "later writes notify at once again");
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
  assert.equal(get(sum), 3, "computed at registration");
  assert.equal(seen, [3]);
  set(a, 10);
  assert.equal(get(sum), 12);
  set(b, 2);
  assert.equal(seen, [3, 12], "a source written with its own value changes nothing");
  transaction(() => {
    set(a, 0);
    set(b, 0);
  });
  assert.equal(get(sum), 0);
  assert.equal(seen, [3, 12, 0], "two source writes in one transaction recompute once");
});

test("a derived key feeds another derived key", () => {
  const a = fresh<number>();
  const twice = fresh<number>();
  const label = fresh<string>();
  set(a, 2);
  derive(twice, [a], (read) => (read(a) ?? 0) * 2);
  derive(label, [twice], (read) => `x${read(twice)}`);
  assert.equal(get(label), "x4");
  set(a, 5);
  assert.equal(get(label), "x10");
});

test("optimistic shows the guess, keeps it on success and puts the key back on failure", async () => {
  const k = fresh<number>();
  set(k, 1);
  const seen: unknown[] = [];
  subscribe(k, (v) => seen.push(v));
  const result = await optimistic(k, 2, async () => "ok");
  assert.equal(result, "ok");
  assert.equal(get(k), 2, "a success leaves the guess for the revalidation to replace");
  await assert.rejects(optimistic(k, 3, async () => Promise.reject(new Error("no"))));
  assert.equal(get(k), 2, "a failure restores what the key held");
  assert.equal(seen, [2, 3, 2]);
});

test("optimistic on a key nothing set clears it again on failure", async () => {
  const k = fresh<number>();
  await assert.rejects(optimistic(k, 7, async () => Promise.reject(new Error("no"))));
  assert.equal(get(k), undefined);
  assert.equal(k in snapshot(), false);
});

test("seed writes a whole map in one transaction and the server's value wins a local one", () => {
  const a = fresh<number>();
  const b = fresh<string>();
  const seen: string[] = [];
  subscribe(a, () => seen.push("a"));
  subscribe(b, () => seen.push("b"));
  set(a, 99);
  seed({ [a]: 1, [b]: "two" });
  assert.equal(get(a), 1);
  assert.equal(get(b), "two");
  assert.equal(seen, ["a", "a", "b"]);
  seed({ [a]: 1 });
  assert.equal(seen, ["a", "a", "b"], "a seed equal to what is held notifies nobody");
});
