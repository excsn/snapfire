import * as L from "__sf_dom__";

const { document } = L.parseHTML("<!doctype html><html><head></head><body></body></html>");
for (const key of Object.keys(L)) {
  if (/^[A-Z]/.test(key) && !(key in globalThis)) globalThis[key] = L[key];
}
globalThis.document = document;
// React decides at import whether `input` events exist by asking the document for `oninput`; without it React falls back to a polyfill that only watches the focused element on keyup, so a controlled text input never sees a change.
const documentProto = Object.getPrototypeOf(document);
if (!("oninput" in documentProto)) documentProto.oninput = null;
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
if (typeof globalThis.DOMParser === "function") {
  const parse = globalThis.DOMParser.prototype.parseFromString;
  globalThis.DOMParser.prototype.parseFromString = function (source, mime) {
    const text = String(source);
    const whole = mime !== "text/html" || /^\s*(<!doctype|<html)/i.test(text);
    return parse.call(this, whole ? text : `<!doctype html><html><head></head><body>${text}</body></html>`, mime);
  };
}
