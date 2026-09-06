import { useHoisted, withHoisted } from "@snapfire/fsr-client/react";
import { assert, render, test } from "@snapfire/fsr-client/testing";

function same(actual: unknown, expected: unknown, message?: string): void {
  assert.equal(JSON.stringify(actual), JSON.stringify(expected), message);
}

let computed: string[] = [];

function Price({ cents }: { cents: number }) {
  const h = useHoisted("src/ui/Price.tsx#Price");
  return <b>{h.r(0, () => (computed.push(`price ${cents}`), `computed ${cents}`))}</b>;
}

function Card({ title, items }: { title: string; items: string[] }) {
  const h = useHoisted("src/ui/Card.tsx#Card");
  return h.c(
    0,
    (html) => <ul className="card" title={title} dangerouslySetInnerHTML={html} />,
    () => (
      <ul className="card" title={title}>
        {items.map((it) => (computed.push(`item ${it}`), <li key={it}>{it}</li>))}
      </ul>
    ),
  );
}

function Bill({ lines }: { lines: number[] }) {
  const h = useHoisted("routes/bill/page.tsx#default");
  return (
    <div>
      <i>{h.r(0, () => (computed.push("total"), "computed total"))}</i>
      <ul>
        {lines.map(
          h.l((cents, i) => (
            <li key={cents}>
              {h.r(1, () => (computed.push(`line ${i}`), `computed line ${i}`))}
              <Price cents={cents} />
            </li>
          )),
        )}
      </ul>
    </div>
  );
}

test("a read takes the server's value under its key and computes only where the server recorded none", async () => {
  computed = [];
  const table = {
    "routes/bill/page.tsx#default|0": "server total",
    "routes/bill/page.tsx#default|1@0": "server line 0",
    "src/ui/Price.tsx#Price|0@0": "server price 0",
    "src/ui/Price.tsx#Price|0@1": "server price 1",
  };
  const r = await render(withHoisted(table, <Bill lines={[100, 200]} />), { hydrate: false });
  assert.equal(r.container.querySelector("i")?.textContent, "server total");
  const items = Array.from(r.container.querySelectorAll("li")).map((li) => li.textContent);
  same(items, ["server line 0server price 0", "computed line 1server price 1"], "the second line's own read missed, its nested Price hit below the caller's index");
  same(computed, ["line 1"], "the misses are the only calls");
  r.unmount();
});

test("without a table every read computes, and a nested component keys below every enclosing loop", async () => {
  computed = [];
  const r = await render(<Bill lines={[7]} />, { hydrate: false });
  assert.equal(r.container.querySelector("i")?.textContent, "computed total");
  assert.equal(r.container.querySelector("li")?.textContent, "computed line 0computed 7");
  same(computed, ["total", "line 0", "price 7"]);
  r.unmount();

  computed = [];
  const table = { "src/ui/Price.tsx#Price|0@1": "server price under index 1" };
  const two = await render(withHoisted(table, <Bill lines={[1, 2]} />), { hydrate: false });
  const prices = Array.from(two.container.querySelectorAll("b")).map((b) => b.textContent);
  same(prices, ["computed 1", "server price under index 1"]);
  two.unmount();
});

test("a keyed list re-renders in place when a new payload reorders it, each line reading the value of its new position", async () => {
  computed = [];
  const before = { "src/ui/Price.tsx#Price|0@0": "server price 10", "src/ui/Price.tsx#Price|0@1": "server price 20" };
  const r = await render(withHoisted(before, <Bill lines={[10, 20]} />), { hydrate: false });
  const first = r.container.querySelectorAll("li")[0];
  const after = { "src/ui/Price.tsx#Price|0@0": "server price 20", "src/ui/Price.tsx#Price|0@1": "server price 10" };
  r.root.render(withHoisted(after, <Bill lines={[20, 10]} />));
  await new Promise((resolve) => setTimeout(resolve, 0));
  const lines = Array.from(r.container.querySelectorAll("li"));
  assert.equal(lines[1], first, "the keyed element moved rather than being recreated");
  same(lines.map((li) => li.querySelector("b")?.textContent), ["server price 20", "server price 10"]);
  assert.equal(computed.filter((c) => c.startsWith("price")).length, 0, "no price was computed on either render");
  r.unmount();
});

test("a static subtree renders the server's markup as inner HTML on a hit and the original JSX on a miss", async () => {
  computed = [];
  const table = { "src/ui/Card.tsx#Card|0": "<li>from</li><li>server</li>" };
  const hit = await render(withHoisted(table, <Card title="t" items={["a", "b"]} />), { hydrate: false });
  const ul = hit.container.querySelector("ul");
  assert.equal(ul?.getAttribute("title"), "t", "the holder keeps its own attributes");
  assert.equal(ul?.innerHTML, "<li>from</li><li>server</li>");
  same(computed, [], "nothing inside the chunk ran");
  hit.unmount();

  const miss = await render(withHoisted({}, <Card title="t" items={["a", "b"]} />), { hydrate: false });
  assert.equal(miss.container.querySelector("ul")?.innerHTML, "<li>a</li><li>b</li>");
  same(computed, ["item a", "item b"]);
  miss.unmount();
});

test("a chunk that unmounts and mounts again reads the table again", async () => {
  computed = [];
  const table = { "src/ui/Card.tsx#Card|0": "<li>kept</li>" };
  function Toggle({ on }: { on: boolean }) {
    return <div>{on ? <Card title="t" items={["x"]} /> : <span>off</span>}</div>;
  }
  const r = await render(withHoisted(table, <Toggle on />), { hydrate: false });
  assert.equal(r.container.querySelector("ul")?.innerHTML, "<li>kept</li>");
  r.root.render(withHoisted(table, <Toggle on={false} />));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(r.container.querySelector("ul"), null);
  r.root.render(withHoisted(table, <Toggle on />));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(r.container.querySelector("ul")?.innerHTML, "<li>kept</li>");
  same(computed, [], "the remount hit too");
  r.unmount();
});
