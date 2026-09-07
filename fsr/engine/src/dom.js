import * as L from "__sf_dom__";

const { document } = L.parseHTML("<!doctype html><html><head></head><body></body></html>");
for (const key of Object.keys(L)) {
  if (/^[A-Z]/.test(key) && !(key in globalThis)) globalThis[key] = L[key];
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
if (typeof globalThis.FormData !== "function") {
  // linkedom has no FormData, and a form handler reading its own submission is
  // ordinary. Named controls only, which is what a submission carries: a
  // checkbox or radio contributes when checked, a disabled control never does,
  // and a multiple select contributes every selected option.
  globalThis.FormData = class FormData {
    constructor(form) {
      this._entries = [];
      if (!form) return;
      for (const el of form.querySelectorAll("input, textarea, select")) {
        const name = el.getAttribute("name");
        if (!name || el.disabled) continue;
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
globalThis.__sf.load = (html, url) => {
  globalThis.__sf_location(url);
  globalThis.document = L.parseHTML(String(html)).document;
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
const rect = () => ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 });
const layout = {
  getClientRects: { value: () => [] },
  getBoundingClientRect: { value: rect },
  scrollIntoView: { value: () => {} },
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
for (const name of ["focus", "blur", "click"]) {
  if (typeof globalThis.HTMLElement.prototype[name] !== "function") {
    globalThis.HTMLElement.prototype[name] = name === "click" ? function () { this.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })); } : function () {};
  }
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
