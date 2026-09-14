import * as L from "__sf_dom__";

const { document } = L.parseHTML("<!doctype html><html><head></head><body></body></html>");
for (const key of Object.keys(L)) {
  if (/^[A-Z]/.test(key) && !(key in globalThis)) globalThis[key] = L[key];
}
// linkedom's classes carry no `Symbol.toStringTag` and the bundle renames some of them (`Element$1`). A browser answers both with the interface name. Libraries decide by it: Vue makes a value reactive when `Object.prototype.toString` calls it a plain object.
for (const [key, value] of Object.entries(L)) {
  if (!/^[A-Z]/.test(key) || typeof value !== "function" || !value.prototype) continue;
  if (Object.prototype.hasOwnProperty.call(value.prototype, Symbol.toStringTag)) continue;
  Object.defineProperty(value.prototype, Symbol.toStringTag, { configurable: true, value: key });
  Object.defineProperty(value, "name", { configurable: true, value: key });
}
globalThis.document = document;
// React decides at import whether `input` events exist by asking the document for `oninput`; without it React falls back to a polyfill that only watches the focused element on keyup, so a controlled text input never sees a change.
const documentProto = Object.getPrototypeOf(document);
if (!("oninput" in documentProto)) documentProto.oninput = null;
// linkedom's `document.title` reads nothing from a parsed document; the navigator sets it on every route change.
Object.defineProperty(documentProto, "title", {
  configurable: true,
  get() {
    return this.querySelector("title")?.textContent ?? "";
  },
  set(value) {
    let el = this.querySelector("title");
    if (!el) {
      el = this.createElement("title");
      (this.head ?? this.documentElement).appendChild(el);
    }
    el.textContent = String(value);
  },
});
// linkedom defines no `readyState`, and a document the runner parsed in one call has no loading phase to be in: code that defers work to `DOMContentLoaded` would wait for an event nothing dispatches.
if (!("readyState" in documentProto)) documentProto.readyState = "complete";
globalThis.window = globalThis;
globalThis.self = globalThis;
if (typeof globalThis.UIEvent !== "function") globalThis.UIEvent = class UIEvent extends Event {};
if (typeof globalThis.MouseEvent !== "function") globalThis.MouseEvent = class MouseEvent extends globalThis.UIEvent {};
if (typeof globalThis.KeyboardEvent !== "function") {
  globalThis.KeyboardEvent = class KeyboardEvent extends globalThis.UIEvent {
    constructor(type, init = {}) {
      super(type, init);
      this.key = init.key ?? "";
      this.code = init.code ?? "";
    }
  };
}
if (typeof globalThis.FocusEvent !== "function") globalThis.FocusEvent = class FocusEvent extends globalThis.UIEvent {};
if (typeof globalThis.InputEvent !== "function") globalThis.InputEvent = class InputEvent extends globalThis.UIEvent {};
// linkedom has no `attachInternals`. A form-associated custom element calls it as it is constructed and gives its form a value through `setFormValue`.
const INTERNALS = new WeakMap();
if (globalThis.HTMLElement && typeof globalThis.HTMLElement.prototype.attachInternals !== "function") {
  class ElementInternals {
    constructor(element) {
      this._element = element;
      this._value = null;
      this._validity = { valid: true };
      this._message = "";
      this.states = new Set();
    }
    get form() {
      return this._element.closest("form");
    }
    get labels() {
      return [];
    }
    get shadowRoot() {
      return this._element.shadowRoot ?? null;
    }
    get willValidate() {
      return true;
    }
    get validity() {
      return this._validity;
    }
    get validationMessage() {
      return this._message;
    }
    setFormValue(value) {
      this._value = value;
    }
    setValidity(flags = {}, message = "") {
      const invalid = Object.values(flags).some(Boolean);
      this._validity = { ...flags, valid: !invalid };
      this._message = invalid ? String(message) : "";
    }
    checkValidity() {
      return this._validity.valid;
    }
    reportValidity() {
      return this._validity.valid;
    }
  }
  globalThis.ElementInternals = ElementInternals;
  globalThis.HTMLElement.prototype.attachInternals = function () {
    if (INTERNALS.has(this)) throw new Error("attachInternals: already attached");
    const internals = new ElementInternals(this);
    INTERNALS.set(this, internals);
    return internals;
  };
}
if (typeof globalThis.FormData !== "function") {
  // linkedom has no FormData. A form handler reading its own submission is
  // ordinary. Named controls only, in tree order, which is what a submission
  // carries: a checkbox or radio contributes when checked, a disabled control
  // never does and a multiple select contributes every selected option. A
  // form-associated custom element contributes what it gave `setFormValue`.
  globalThis.FormData = class FormData {
    constructor(form) {
      this._entries = [];
      if (!form) return;
      for (const el of form.querySelectorAll("*")) {
        const name = el.getAttribute("name");
        if (!name || el.disabled || el.hasAttribute("disabled")) continue;
        const internals = INTERNALS.get(el);
        if (internals) {
          if (!el.constructor.formAssociated || internals._value == null) continue;
          if (internals._value instanceof FormData) for (const entry of internals._value.entries()) this._entries.push(entry);
          else this._entries.push([name, String(internals._value)]);
          continue;
        }
        if (el.tagName !== "INPUT" && el.tagName !== "TEXTAREA" && el.tagName !== "SELECT") continue;
        const type = (el.getAttribute("type") || "").toLowerCase();
        if ((type === "checkbox" || type === "radio") && !el.checked) continue;
        if (el.tagName === "SELECT" && el.multiple) {
          for (const option of el.querySelectorAll("option")) {
            if (option.selected) this._entries.push([name, String(option.value)]);
          }
          continue;
        }
        this._entries.push([name, String(el.value ?? "")]);
      }
    }
    get(name) {
      const found = this._entries.find(([key]) => key === name);
      return found ? found[1] : null;
    }
    getAll(name) {
      return this._entries.filter(([key]) => key === name).map(([, value]) => value);
    }
    has(name) {
      return this._entries.some(([key]) => key === name);
    }
    append(name, value) {
      this._entries.push([String(name), String(value)]);
    }
    set(name, value) {
      const at = this._entries.findIndex(([key]) => key === name);
      const entry = [String(name), String(value)];
      if (at === -1) this._entries.push(entry);
      else this._entries[at] = entry;
    }
    delete(name) {
      this._entries = this._entries.filter(([key]) => key !== name);
    }
    keys() {
      return this._entries.map(([key]) => key)[Symbol.iterator]();
    }
    values() {
      return this._entries.map(([, value]) => value)[Symbol.iterator]();
    }
    entries() {
      return this._entries.map((pair) => pair.slice())[Symbol.iterator]();
    }
    forEach(f, thisArg) {
      for (const [key, value] of this._entries) f.call(thisArg, value, key, this);
    }
    [Symbol.iterator]() {
      return this.entries();
    }
  };
}
globalThis.addEventListener = (...a) => globalThis.document.addEventListener(...a);
globalThis.removeEventListener = (...a) => globalThis.document.removeEventListener(...a);
if (new globalThis.MouseEvent("click").button !== 0) {
  const Base = globalThis.MouseEvent;
  globalThis.MouseEvent = class MouseEvent extends Base {
    constructor(type, init = {}) {
      super(type, init);
      this.button = init.button ?? 0;
      this.buttons = init.buttons ?? 0;
      this.clientX = init.clientX ?? 0;
      this.clientY = init.clientY ?? 0;
      this.metaKey = !!init.metaKey;
      this.ctrlKey = !!init.ctrlKey;
      this.shiftKey = !!init.shiftKey;
      this.altKey = !!init.altKey;
    }
  };
}
// A browser adopts a node inserted from another document: the node takes the new one as its owner. linkedom does not. A module that captured `document` at import keeps creating nodes in the first page's document (Vue's runtime does), so without adoption the focus such a node records lands on that document.
const INSERTS = ["appendChild", "insertBefore", "replaceChild", "append", "prepend", "before", "after", "replaceWith", "replaceChildren"];
const adopt = (node, doc) => {
  node.ownerDocument = doc;
  for (let child = node.firstChild; child; child = child.nextSibling) adopt(child, doc);
};
if (typeof document.createElement === "function") {
  const adopting = new Map();
  const wrappers = new WeakSet();
  for (const sample of [document, document.createElement("div"), document.createTextNode(""), document.createComment(""), document.createDocumentFragment()]) {
    for (let proto = Object.getPrototypeOf(sample); proto && proto !== Object.prototype; proto = Object.getPrototypeOf(proto)) {
      for (const name of INSERTS) {
        const own = Object.getOwnPropertyDescriptor(proto, name);
        if (!own || typeof own.value !== "function" || wrappers.has(own.value)) continue;
        let wrapper = adopting.get(own.value);
        if (!wrapper) {
          const insert = own.value;
          wrapper = function (...nodes) {
            const doc = this.nodeType === 9 ? this : this.ownerDocument;
            for (const node of nodes) {
              if (node && typeof node === "object" && node.ownerDocument && node.ownerDocument !== doc) adopt(node, doc);
            }
            return insert.apply(this, nodes);
          };
          adopting.set(insert, wrapper);
          wrappers.add(wrapper);
        }
        Object.defineProperty(proto, name, { ...own, value: wrapper });
      }
    }
  }
}
// linkedom keeps the custom element registry on each document while the modules that define elements run once, so every page takes the first registry and has its elements upgraded against it.
const registry = typeof document.createElement === "function" ? document.defaultView?.customElements : undefined;
const registryKey = registry ? Object.getOwnPropertySymbols(document).find((key) => document[key] === registry) : undefined;
// A browser reports a constructor that throws during an upgrade and goes on; linkedom lets it escape `define`, which aborts the module defining the element.
if (registry) {
  const registryProto = Object.getPrototypeOf(registry);
  const upgrade = registryProto.upgrade;
  registryProto.upgrade = function (element) {
    try {
      return upgrade.call(this, element);
    } catch (err) {
      console.error(err);
    }
  };
}
// In a browser a page's modules run again and take the new document. Here they stay loaded, so a document one of them still holds answers its lookups from the page showing now.
const LOOKUPS = ["querySelector", "querySelectorAll", "getElementById", "getElementsByClassName", "getElementsByTagName", "getElementsByName"];
const retire = (doc) => {
  for (const name of LOOKUPS) {
    if (typeof doc[name] === "function") Object.defineProperty(doc, name, { configurable: true, value: (...args) => globalThis.document[name](...args) });
  }
  for (const name of ["body", "head", "activeElement"]) Object.defineProperty(doc, name, { configurable: true, get: () => globalThis.document[name] });
};
globalThis.__sf.load = (html, url) => {
  globalThis.__sf_location(url);
  const page = L.parseHTML(String(html)).document;
  retire(globalThis.document);
  globalThis.document = page;
  if (registryKey) {
    page[registryKey] = registry;
    registry.ownerDocument = page;
    if (registry.active) {
      for (const el of page.querySelectorAll("*")) if (registry.get(el.localName)) registry.upgrade(el);
    }
  }
  selectable(globalThis.document);
  const nested = globalThis.document.documentElement.querySelector("html");
  if (nested) {
    for (const name of nested.getAttributeNames()) globalThis.document.documentElement.setAttribute(name, nested.getAttribute(name));
  }
};
globalThis.IntersectionObserver = class IntersectionObserver {
  constructor(cb) {
    this.cb = cb;
  }
  observe(el) {
    setTimeout(() => this.cb([{ isIntersecting: true, target: el }]), 0);
  }
  disconnect() {}
  unobserve() {}
};
globalThis.requestIdleCallback = (fn) => setTimeout(fn, 0);
// linkedom keeps the custom element registry on `document.defaultView`; a module reads it as a global and the document is replaced on every load.
if (!("customElements" in globalThis)) {
  Object.defineProperty(globalThis, "customElements", { configurable: true, get: () => globalThis.document.defaultView.customElements });
}
// linkedom has no XPath. A library that compiles an expression at import, htmx for one, gets one that matches nothing.
if (typeof globalThis.XPathEvaluator !== "function") {
  globalThis.XPathEvaluator = class XPathEvaluator {
    createExpression() {
      return { evaluate: () => ({ iterateNext: () => null }) };
    }
  };
}
const rect = () => ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 });
const layout = {
  getClientRects: { value: () => [], writable: true },
  getBoundingClientRect: { value: rect, writable: true },
  scrollIntoView: { value: () => {}, writable: true },
  offsetWidth: { get: () => 0 },
  offsetHeight: { get: () => 0 },
  offsetTop: { get: () => 0 },
  offsetLeft: { get: () => 0 },
  clientWidth: { get: () => 0 },
  clientHeight: { get: () => 0 },
  scrollWidth: { get: () => 0 },
  scrollHeight: { get: () => 0 },
  scrollTop: { get: () => 0, set: () => {} },
  scrollLeft: { get: () => 0, set: () => {} },
};
for (const [name, descriptor] of Object.entries(layout)) {
  if (!(name in globalThis.Element.prototype)) Object.defineProperty(globalThis.Element.prototype, name, { configurable: true, ...descriptor });
}
if (typeof globalThis.HTMLElement.prototype.click !== "function") {
  globalThis.HTMLElement.prototype.click = function () {
    this.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  };
}
// linkedom tracks no focus. `document.activeElement` is the element `focus()`
// last reached while it is still in the document, else the body; `focus` and
// `blur` move it and dispatch what a browser does, `focusin` and `focusout`
// bubbling, since React listens for those.
const FOCUSED = Symbol("sf.focused");
Object.defineProperty(documentProto, "activeElement", {
  configurable: true,
  get() {
    const el = this[FOCUSED];
    return el && el.isConnected ? el : (this.body ?? null);
  },
});
if (!("hasFocus" in documentProto)) documentProto.hasFocus = () => true;
const focusEvent = (type, bubbles, related) => {
  const event = new globalThis.FocusEvent(type, { bubbles, cancelable: false });
  Object.defineProperty(event, "relatedTarget", { value: related, configurable: true });
  return event;
};
globalThis.HTMLElement.prototype.focus = function () {
  const doc = this.ownerDocument;
  const was = doc[FOCUSED] && doc[FOCUSED].isConnected ? doc[FOCUSED] : null;
  if (was === this || !this.isConnected) return;
  doc[FOCUSED] = this;
  if (was) {
    was.dispatchEvent(focusEvent("blur", false, this));
    was.dispatchEvent(focusEvent("focusout", true, this));
  }
  this.dispatchEvent(focusEvent("focus", false, was));
  this.dispatchEvent(focusEvent("focusin", true, was));
};
globalThis.HTMLElement.prototype.blur = function () {
  const doc = this.ownerDocument;
  if (doc[FOCUSED] !== this) return;
  doc[FOCUSED] = null;
  this.dispatchEvent(focusEvent("blur", false, null));
  this.dispatchEvent(focusEvent("focusout", true, null));
};
// linkedom keeps an input's value in its `value` attribute, so setting one
// sets the other. A browser keeps them apart once the value is set: the
// attribute is the default the markup gave and the property is what the
// control holds, so the markup can change under someone typing without
// taking what they typed.
const HELD = Symbol("sf.value");
const inputValue = globalThis.HTMLInputElement && Object.getOwnPropertyDescriptor(globalThis.HTMLInputElement.prototype, "value");
if (inputValue?.get && inputValue.set) {
  Object.defineProperty(globalThis.HTMLInputElement.prototype, "value", {
    configurable: true,
    get() {
      return HELD in this ? this[HELD] : inputValue.get.call(this);
    },
    set(value) {
      this[HELD] = value === null || value === undefined ? "" : String(value);
    },
  });
}
// An input with no `type` or with one a browser does not know is a text
// input. linkedom answers the attribute as it stands. React reads `type` to
// decide whether an `input` event is a change.
const INPUT_TYPES = new Set(["button", "checkbox", "color", "date", "datetime-local", "email", "file", "hidden", "image", "month", "number", "password", "radio", "range", "reset", "search", "submit", "tel", "text", "time", "url", "week"]);
if (globalThis.HTMLInputElement) {
  Object.defineProperty(globalThis.HTMLInputElement.prototype, "type", {
    configurable: true,
    get() {
      const type = (this.getAttribute("type") ?? "").toLowerCase();
      return INPUT_TYPES.has(type) ? type : "text";
    },
    set(value) {
      this.setAttribute("type", String(value));
    },
  });
}
// linkedom tracks no selection. React reads the focused control's on every
// event. A text control's selection is its caret, at the end of its value
// unless `setSelectionRange` moved it; anywhere else the document's
// selection is empty.
const SELECTION = Symbol("sf.selection");
for (const Ctor of [globalThis.HTMLInputElement, globalThis.HTMLTextAreaElement]) {
  if (!Ctor || "selectionStart" in Ctor.prototype) continue;
  const length = (el) => String(el.value ?? "").length;
  Object.defineProperty(Ctor.prototype, "selectionStart", {
    configurable: true,
    get() {
      return this[SELECTION]?.[0] ?? length(this);
    },
    set(start) {
      this[SELECTION] = [Number(start), this.selectionEnd];
    },
  });
  Object.defineProperty(Ctor.prototype, "selectionEnd", {
    configurable: true,
    get() {
      return this[SELECTION]?.[1] ?? length(this);
    },
    set(end) {
      this[SELECTION] = [this.selectionStart, Number(end)];
    },
  });
  Ctor.prototype.setSelectionRange = function (start, end) {
    this[SELECTION] = [Number(start), Number(end)];
  };
  if (typeof Ctor.prototype.select !== "function") {
    Ctor.prototype.select = function () {
      this[SELECTION] = [0, length(this)];
    };
  }
}
const emptySelection = { anchorNode: null, anchorOffset: 0, focusNode: null, focusOffset: 0, rangeCount: 0, isCollapsed: true, type: "None", removeAllRanges() {}, addRange() {}, collapse() {}, extend() {}, toString: () => "" };
const selectable = (doc) => {
  const view = doc.defaultView;
  if (view && typeof view.getSelection !== "function") {
    try {
      view.getSelection = () => emptySelection;
    } catch {}
  }
};
if (typeof globalThis.getSelection !== "function") globalThis.getSelection = () => emptySelection;
if (typeof documentProto.getSelection !== "function") documentProto.getSelection = () => emptySelection;
selectable(document);
// linkedom's button has no `value`. A browser's reflects the attribute and a
// click carries it as the target's value.
if (globalThis.HTMLButtonElement && !Object.getOwnPropertyDescriptor(globalThis.HTMLButtonElement.prototype, "value")) {
  Object.defineProperty(globalThis.HTMLButtonElement.prototype, "value", {
    configurable: true,
    get() {
      return this.getAttribute("value") ?? "";
    },
    set(value) {
      this.setAttribute("value", String(value));
    },
  });
}
// linkedom gives no control a `form`. A browser's is the form its `form`
// attribute names by id, else the form it sits in.
for (const name of ["HTMLButtonElement", "HTMLInputElement", "HTMLSelectElement", "HTMLTextAreaElement", "HTMLFieldSetElement", "HTMLOutputElement", "HTMLObjectElement"]) {
  const Ctor = globalThis[name];
  if (!Ctor || "form" in Ctor.prototype) continue;
  Object.defineProperty(Ctor.prototype, "form", {
    configurable: true,
    get() {
      const id = this.getAttribute("form");
      if (id === null) return this.closest("form");
      const named = this.ownerDocument?.getElementById(id);
      return named?.tagName === "FORM" ? named : null;
    },
  });
}
// linkedom's form has no `requestSubmit`. A browser's fires a cancelable
// `submit` naming the submitter.
if (globalThis.HTMLFormElement && typeof globalThis.HTMLFormElement.prototype.requestSubmit !== "function") {
  globalThis.HTMLFormElement.prototype.requestSubmit = function (submitter) {
    const event = new Event("submit", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "submitter", { value: submitter ?? null, configurable: true });
    this.dispatchEvent(event);
  };
}
// A form handler that clears its own fields after keeping them is ordinary,
// and linkedom has no `reset`. Each control goes back to the default the
// markup gave it, which is what a browser does.
if (typeof globalThis.HTMLElement.prototype.reset !== "function") {
  globalThis.HTMLElement.prototype.reset = function () {
    if (this.tagName !== "FORM") return;
    for (const el of this.querySelectorAll("input, textarea, select")) {
      const type = (el.getAttribute("type") || "").toLowerCase();
      if (type === "checkbox" || type === "radio") {
        el.checked = el.hasAttribute("checked");
        continue;
      }
      if (el.tagName === "SELECT") {
        for (const option of el.querySelectorAll("option")) option.selected = option.hasAttribute("selected");
        continue;
      }
      el.value = el.getAttribute(el.tagName === "TEXTAREA" ? "value" : "value") ?? el.textContent ?? "";
    }
    this.dispatchEvent(new Event("reset", { bubbles: true, cancelable: true }));
  };
}
if (typeof globalThis.DOMParser === "function") {
  const parse = globalThis.DOMParser.prototype.parseFromString;
  globalThis.DOMParser.prototype.parseFromString = function (source, mime) {
    const text = String(source);
    const whole = mime !== "text/html" || /^\s*(<!doctype|<html)/i.test(text);
    return parse.call(this, whole ? text : `<!doctype html><html><head></head><body>${text}</body></html>`, mime);
  };
}
