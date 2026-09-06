import { morph } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

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
  assert.equal(el.querySelector("p"), p, "the element stays");
  assert.equal(p?.firstChild, text, "and so does its text node");
  assert.equal(p?.textContent, "uno");
  assert.equal(p?.getAttribute("class"), "b");
  assert.equal(p?.hasAttribute("title"), false, "a removed attribute goes");
  assert.equal(el.children.length, 3);
  morph(el, "<p>uno</p>");
  assert.equal(el.children.length, 1, "trailing nodes are removed");
  el.remove();
});

test("a keyed element moves rather than being recreated, and an unkeyed one is matched by position", () => {
  const el = box('<ul><li data-sf-key="a">a</li><li data-sf-key="b">b</li><li data-sf-key="c">c</li></ul>');
  const [a, b, c] = Array.from(el.querySelectorAll("li"));
  morph(el, '<ul><li data-sf-key="c">c</li><li data-sf-key="a">a!</li><li data-sf-key="b">b</li></ul>');
  const after = Array.from(el.querySelectorAll("li"));
  assert.equal(after[0], c);
  assert.equal(after[1], a);
  assert.equal(after[2], b);
  assert.equal(a.textContent, "a!");
  morph(el, "<ul><li>x</li><li>y</li></ul>");
  assert.equal(el.querySelectorAll("li").length, 2);
  el.remove();
});

test("a focused control keeps what the user typed; an unfocused one takes the server's value", () => {
  const el = box('<input name="q" value="server"><input name="r" value="server">');
  const [q, r] = Array.from(el.querySelectorAll("input"));
  q.value = "typing";
  q.focus();
  r.value = "stale";
  morph(el, '<input name="q" value="server2"><input name="r" value="server2">');
  assert.equal(q.value, document.activeElement === q ? "typing" : "server2", "kept while focused; the runner's DOM may not track focus, and then it follows the server");
  assert.equal(r.value, "server2");
  assert.equal(q.getAttribute("value"), "server2", "the attribute still follows the server");
  el.remove();
});

test("a nested island inside the markup is left as it stands", () => {
  const el = box('<div><sf-i id="sf-i9" data-sf-module="m"><b>mounted</b></sf-i></div>');
  const inner = el.querySelector("sf-i");
  morph(el, '<div><sf-i id="sf-i9" data-sf-module="m"><b>server</b></sf-i></div>');
  assert.equal(el.querySelector("sf-i"), inner);
  assert.equal(inner?.textContent, "mounted");
  el.remove();
});
