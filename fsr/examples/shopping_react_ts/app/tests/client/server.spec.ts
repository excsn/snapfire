import { morph } from "@snapfire/fsr-client";
import { expect, test } from "@snapfire/fsr-client/testing";

function box(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

test("morph changes text and attributes in place and keeps the nodes it can", () => {
  const el = box('<p class="a" title="t">one</p><span>two</span>');
  const p = el.querySelector("p");
  const text = p?.firstChild;
  morph(el, '<p class="b">uno</p><span>two</span><i>three</i>');
  expect(el.querySelector("p"), "the element stays").toBe(p);
  expect(p?.firstChild, "and so does its text node").toBe(text);
  expect(p?.textContent).toEqual("uno");
  expect(p?.getAttribute("class")).toEqual("b");
  expect(p?.hasAttribute("title"), "a removed attribute goes").toEqual(false);
  expect(el.children.length).toEqual(3);
  morph(el, "<p>uno</p>");
  expect(el.children.length, "trailing nodes are removed").toEqual(1);
  el.remove();
});

test("a keyed element moves rather than being recreated, and an unkeyed one is matched by position", () => {
  const el = box('<ul><li data-sf-key="a">a</li><li data-sf-key="b">b</li><li data-sf-key="c">c</li></ul>');
  const [a, b, c] = Array.from(el.querySelectorAll("li"));
  morph(el, '<ul><li data-sf-key="c">c</li><li data-sf-key="a">a!</li><li data-sf-key="b">b</li></ul>');
  const after = Array.from(el.querySelectorAll("li"));
  expect(after[0]).toBe(c);
  expect(after[1]).toBe(a);
  expect(after[2]).toBe(b);
  expect(a.textContent).toEqual("a!");
  morph(el, "<ul><li>x</li><li>y</li></ul>");
  expect(el.querySelectorAll("li").length).toEqual(2);
  el.remove();
});

test("a focused control keeps what the user typed; an unfocused one takes the server's value", () => {
  const el = box('<input name="q" value="server"><input name="r" value="server">');
  const [q, r] = Array.from(el.querySelectorAll("input"));
  q.value = "typing";
  q.focus();
  r.value = "stale";
  morph(el, '<input name="q" value="server2"><input name="r" value="server2">');
  expect(document.activeElement).toBe(q);
  expect(q.value, "kept while focused").toEqual("typing");
  expect(r.value).toEqual("server2");
  expect(q.getAttribute("value"), "the attribute still follows the server").toEqual("server2");
  el.remove();
});

test("a nested island inside the markup is left as it stands", () => {
  const el = box('<div><sf-i id="sf-i9" data-sf-module="m"><b>mounted</b></sf-i></div>');
  const inner = el.querySelector("sf-i");
  morph(el, '<div><sf-i id="sf-i9" data-sf-module="m"><b>server</b></sf-i></div>');
  expect(el.querySelector("sf-i")).toBe(inner);
  expect(inner?.textContent).toEqual("mounted");
  el.remove();
});

test("an island region moves with its key rather than taking the markup of whatever is now in its place", () => {
  const el = box('<sf-s data-sf-island data-sf-region="r|a"><span>a</span></sf-s>');
  const held = el.firstElementChild;
  morph(el, '<sf-s data-sf-island data-sf-region="r|b"><span>b</span></sf-s><sf-s data-sf-island data-sf-region="r|a"><span>a</span></sf-s>');
  expect(el.children[1], "the region already here is the second one now").toBe(held);
  expect(el.children[0].getAttribute("data-sf-region")).toEqual("r|b");
  expect(el.children[1].getAttribute("data-sf-region")).toEqual("r|a");
});
