import { settle, show } from "./harness.js";
import { controlOf, isInaccessible } from "./queries.js";

/** Sets a form control's value the way a user would, past React's own value tracking, so the `input` event that follows is seen as a change. */
export function setValue(el: HTMLElement, value: string): void {
  if (el.tagName === "SELECT") {
    const option = Array.from(el.querySelectorAll("option")).find((o) => String(o.value) === value);
    if (!option) throw new Error(`no option with value ${show(value)}`);
    option.selected = true;
    return;
  }
  const proto = Object.getPrototypeOf(el) as object;
  const descriptor = Object.getOwnPropertyDescriptor(proto, "value");
  if (descriptor?.set) {
    descriptor.set.call(el, value);
  } else {
    (el as unknown as { value: string }).value = value;
  }
}

type Target = Element | Document | Window;

/** What an event is made with: its init fields, plus `target`, properties set on the element before it is dispatched, as in `fireEvent.change(input, { target: { value } })`. */
export type EventInit = Record<string, unknown> & { target?: Record<string, unknown> };

/** Each event `fireEvent` names: the DOM type, its constructor, whether it bubbles and whether it can be cancelled. */
const TABLE: Record<string, [string, string, boolean, boolean]> = {
  click: ["click", "MouseEvent", true, true],
  dblClick: ["dblclick", "MouseEvent", true, true],
  mouseDown: ["mousedown", "MouseEvent", true, true],
  mouseUp: ["mouseup", "MouseEvent", true, true],
  mouseMove: ["mousemove", "MouseEvent", true, true],
  mouseOver: ["mouseover", "MouseEvent", true, true],
  mouseOut: ["mouseout", "MouseEvent", true, true],
  mouseEnter: ["mouseenter", "MouseEvent", false, false],
  mouseLeave: ["mouseleave", "MouseEvent", false, false],
  contextMenu: ["contextmenu", "MouseEvent", true, true],
  pointerDown: ["pointerdown", "PointerEvent", true, true],
  pointerUp: ["pointerup", "PointerEvent", true, true],
  pointerMove: ["pointermove", "PointerEvent", true, true],
  pointerOver: ["pointerover", "PointerEvent", true, true],
  pointerOut: ["pointerout", "PointerEvent", true, true],
  pointerEnter: ["pointerenter", "PointerEvent", false, false],
  pointerLeave: ["pointerleave", "PointerEvent", false, false],
  pointerCancel: ["pointercancel", "PointerEvent", true, false],
  keyDown: ["keydown", "KeyboardEvent", true, true],
  keyUp: ["keyup", "KeyboardEvent", true, true],
  keyPress: ["keypress", "KeyboardEvent", true, true],
  focus: ["focus", "FocusEvent", false, false],
  blur: ["blur", "FocusEvent", false, false],
  focusIn: ["focusin", "FocusEvent", true, false],
  focusOut: ["focusout", "FocusEvent", true, false],
  input: ["input", "InputEvent", true, false],
  change: ["change", "Event", true, false],
  submit: ["submit", "Event", true, true],
  reset: ["reset", "Event", true, true],
  invalid: ["invalid", "Event", false, true],
  select: ["select", "Event", true, false],
  scroll: ["scroll", "UIEvent", false, false],
  wheel: ["wheel", "WheelEvent", true, true],
  touchStart: ["touchstart", "TouchEvent", true, true],
  touchEnd: ["touchend", "TouchEvent", true, true],
  touchMove: ["touchmove", "TouchEvent", true, true],
  touchCancel: ["touchcancel", "TouchEvent", true, false],
  dragStart: ["dragstart", "DragEvent", true, true],
  drag: ["drag", "DragEvent", true, true],
  dragEnd: ["dragend", "DragEvent", true, false],
  dragEnter: ["dragenter", "DragEvent", true, true],
  dragLeave: ["dragleave", "DragEvent", true, false],
  dragOver: ["dragover", "DragEvent", true, true],
  drop: ["drop", "DragEvent", true, true],
  copy: ["copy", "ClipboardEvent", true, true],
  cut: ["cut", "ClipboardEvent", true, true],
  paste: ["paste", "ClipboardEvent", true, true],
  load: ["load", "Event", false, false],
  error: ["error", "Event", false, false],
  animationStart: ["animationstart", "AnimationEvent", true, false],
  animationEnd: ["animationend", "AnimationEvent", true, false],
  transitionEnd: ["transitionend", "TransitionEvent", true, true],
};

/** The constructors linkedom lacks stand in for the nearest it has. */
const FALLBACK: Record<string, string> = { PointerEvent: "MouseEvent", WheelEvent: "MouseEvent", DragEvent: "MouseEvent", TouchEvent: "UIEvent", ClipboardEvent: "Event", AnimationEvent: "Event", TransitionEvent: "Event" };

function construct(type: string, ctor: string, init: Record<string, unknown>): Event {
  const globals = globalThis as unknown as Record<string, typeof Event | undefined>;
  const Ctor = globals[ctor] ?? globals[FALLBACK[ctor] ?? "Event"] ?? Event;
  const event = new Ctor(type, init as never);
  for (const [key, value] of Object.entries(init)) {
    if ((event as unknown as Record<string, unknown>)[key] === value) continue;
    try {
      Object.defineProperty(event, key, { value, configurable: true });
    } catch {}
  }
  return event;
}

function isElement(node: Target): node is Element {
  return (node as Node).nodeType === 1;
}

function applyTarget(el: Element, target: Record<string, unknown>): void {
  for (const [key, value] of Object.entries(target)) {
    if (key === "value") setValue(el as HTMLElement, String(value));
    else if (key === "files") Object.defineProperty(el, "files", { value, configurable: true });
    else (el as unknown as Record<string, unknown>)[key] = value;
  }
}

/** An event of the kind `name` names, ready to dispatch on `node`. */
export function createEvent(name: string, node: Target, init: EventInit = {}): Event {
  const spec = TABLE[name];
  if (!spec) throw new Error(`createEvent: no event named ${JSON.stringify(name)}`);
  const { target, ...rest } = init;
  if (target && isElement(node)) applyTarget(node, target);
  return construct(spec[0], spec[1], { bubbles: spec[2], cancelable: spec[3], ...(spec[0] === "click" ? { button: 0, detail: 1 } : {}), ...rest });
}

const CONTROLS = ["BUTTON", "INPUT", "SELECT", "TEXTAREA", "OPTGROUP", "OPTION", "FIELDSET"];

function isDisabled(el: Element): boolean {
  if (!CONTROLS.includes(el.tagName)) return false;
  if (el.hasAttribute("disabled")) return true;
  for (let up = el.parentElement; up; up = up.parentElement) {
    if (up.tagName === "FIELDSET" && up.hasAttribute("disabled")) {
      const legend = up.querySelector(":scope > legend");
      if (!legend || !legend.contains(el)) return true;
    }
  }
  return false;
}

function typeOf(el: Element): string {
  return (el.getAttribute("type") ?? (el.tagName === "BUTTON" ? "submit" : "text")).toLowerCase();
}

function formOf(el: Element): Element | null {
  const named = el.getAttribute("form");
  return named ? el.ownerDocument.getElementById(named) : el.closest("form");
}

function submitForm(form: Element, submitter: Element | null): void {
  const event = construct("submit", "Event", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "submitter", { value: submitter, configurable: true });
  form.dispatchEvent(event);
}

/** A click's effect before listeners see it, the way a browser runs it: a checkbox toggles and a radio checks. The returned function undoes it when a listener cancels the click. */
function preActivate(el: Element): (() => void) | null {
  if (el.tagName !== "INPUT") return null;
  const input = el as HTMLInputElement;
  const type = typeOf(el);
  if (type === "checkbox") {
    const was = input.checked;
    input.checked = !was;
    return () => {
      input.checked = was;
    };
  }
  if (type === "radio" && !input.checked) {
    const name = el.getAttribute("name");
    const scope = formOf(el) ?? el.ownerDocument;
    const group = name ? Array.from(scope.querySelectorAll(`input[type=radio][name="${name}"]`)) : [];
    const before = group.find((r) => (r as HTMLInputElement).checked) as HTMLInputElement | undefined;
    input.checked = true;
    if (before && before !== input) before.checked = false;
    return () => {
      input.checked = false;
      if (before) before.checked = true;
    };
  }
  return null;
}

/** A click's effect after listeners let it stand: a label forwards it to its control, a submit button submits its form and a reset button resets it. */
function activate(el: Element): void {
  const label = el.closest("label");
  if (label) {
    const control = controlOf(label);
    if (control && control !== el && !control.contains(el) && !isDisabled(control)) click(control);
  }
  const button = el.closest("button, input");
  if (!button || isDisabled(button) || (button.tagName === "INPUT" && !["submit", "image", "reset"].includes(typeOf(button)))) return;
  const form = formOf(button);
  if (!form) return;
  const type = typeOf(button);
  if (type === "submit" || type === "image") submitForm(form, button);
  else if (type === "reset") (form as HTMLFormElement).reset?.();
}

/** Dispatches a click with a browser's default action around it. False when a listener cancelled it. */
function click(el: Element, init: Record<string, unknown> = {}): boolean {
  const revert = preActivate(el);
  const ok = el.dispatchEvent(createEvent("click", el, init));
  if (!ok) {
    revert?.();
    return false;
  }
  activate(el);
  return true;
}

type Fire = (node: Target, init?: EventInit | string) => Promise<boolean>;

export type FireEvent = ((node: Target, event: Event) => Promise<boolean>) & { [name in keyof typeof TABLE]: Fire };

function method(name: string): Fire {
  return async (node, init) => {
    let ok: boolean;
    const el = isElement(node) ? node : null;
    switch (name) {
      case "click":
        ok = el ? click(el, typeof init === "object" ? init : {}) : node.dispatchEvent(createEvent(name, node));
        break;
      case "change":
        if (typeof init === "string") {
          setValue(node as HTMLElement, init);
          node.dispatchEvent(createEvent("input", node));
          ok = node.dispatchEvent(createEvent("change", node));
        } else ok = node.dispatchEvent(createEvent(name, node, init));
        break;
      case "keyDown":
      case "keyUp":
      case "keyPress":
        ok = node.dispatchEvent(createEvent(name, node, typeof init === "string" ? { key: init } : init));
        break;
      case "submit": {
        const form = el && el.tagName !== "FORM" ? (el.closest("form") ?? el) : node;
        ok = form.dispatchEvent(createEvent(name, form, typeof init === "object" ? init : {}));
        break;
      }
      case "mouseEnter":
      case "mouseLeave":
      case "pointerEnter":
      case "pointerLeave":
      case "focus":
      case "blur": {
        const paired: Record<string, string> = { mouseEnter: "mouseOver", mouseLeave: "mouseOut", pointerEnter: "pointerOver", pointerLeave: "pointerOut", focus: "focusIn", blur: "focusOut" };
        const initObject = typeof init === "object" ? init : {};
        ok = node.dispatchEvent(createEvent(name, node, initObject));
        node.dispatchEvent(createEvent(paired[name], node, initObject));
        break;
      }
      default:
        ok = node.dispatchEvent(createEvent(name, node, typeof init === "object" ? init : {}));
    }
    await settle();
    return ok;
  };
}

/** Dispatches DOM events and settles the engine after each, so the assertion that follows sees the re-render. `change` also takes the new value as a string. A click runs a browser's default action: a checkbox toggles, a label clicks its control and a submit button submits its form. */
export const fireEvent: FireEvent = Object.assign(
  async (node: Target, event: Event): Promise<boolean> => {
    const ok = node.dispatchEvent(event);
    await settle();
    return ok;
  },
  Object.fromEntries(Object.keys(TABLE).map((name) => [name, method(name)])),
) as FireEvent;

// ---- userEvent ----

export interface UserOptions {
  /** Ignored: the harness clock moves only when a test moves it, so there is nothing to wait between keys. */
  delay?: number | null;
  /** Skips the hover a click begins with. */
  skipHover?: boolean;
  /** Ignored, since time never passes on its own. */
  advanceTimers?: (ms: number) => unknown;
}

export interface UserEvent {
  click(element: Element, options?: { skipHover?: boolean }): Promise<void>;
  dblClick(element: Element): Promise<void>;
  tripleClick(element: Element): Promise<void>;
  hover(element: Element): Promise<void>;
  unhover(element: Element): Promise<void>;
  tab(options?: { shift?: boolean }): Promise<void>;
  type(element: Element, text: string, options?: { skipClick?: boolean }): Promise<void>;
  keyboard(text: string): Promise<void>;
  clear(element: Element): Promise<void>;
  selectOptions(element: Element, values: string | Element | (string | Element)[]): Promise<void>;
  deselectOptions(element: Element, values: string | Element | (string | Element)[]): Promise<void>;
  upload(element: Element, files: unknown | unknown[]): Promise<void>;
  paste(text: string): Promise<void>;
}

const TEXT_TYPES = ["text", "search", "email", "tel", "url", "password", "number", "date", "datetime-local", "month", "time", "week", ""];

function isEditable(el: Element): boolean {
  if (isDisabled(el) || el.hasAttribute("readonly")) return false;
  if (el.tagName === "TEXTAREA") return true;
  if (el.tagName === "INPUT") return TEXT_TYPES.includes(typeOf(el));
  return el.getAttribute("contenteditable") === "true" || el.getAttribute("contenteditable") === "";
}

function isFocusable(el: Element): boolean {
  if (isDisabled(el) || isInaccessible(el)) return false;
  switch (el.tagName) {
    case "A":
    case "AREA":
      return el.hasAttribute("href");
    case "BUTTON":
    case "SELECT":
    case "TEXTAREA":
    case "IFRAME":
    case "SUMMARY":
      return true;
    case "INPUT":
      return typeOf(el) !== "hidden";
    default:
      return el.hasAttribute("tabindex") || el.getAttribute("contenteditable") === "true" || el.getAttribute("contenteditable") === "";
  }
}

function focusOn(el: Element | null): void {
  for (let at = el; at; at = at.parentElement) {
    if (isFocusable(at)) {
      (at as HTMLElement).focus();
      return;
    }
  }
  const active = document.activeElement as HTMLElement | null;
  if (active && active !== document.body) active.blur?.();
}

function active(): Element {
  return document.activeElement ?? document.body;
}

function insertText(el: Element, text: string): void {
  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") {
    const input = el as HTMLInputElement;
    const current = String(input.value ?? "");
    const max = Number(el.getAttribute("maxlength") ?? "-1");
    const next = max >= 0 ? (current + text).slice(0, max) : current + text;
    if (next === current) return;
    setValue(input, next);
  } else {
    el.appendChild(el.ownerDocument.createTextNode(text));
  }
  el.dispatchEvent(construct("input", "InputEvent", { bubbles: true, inputType: "insertText", data: text }));
}

function deleteBackward(el: Element): void {
  if (el.tagName !== "INPUT" && el.tagName !== "TEXTAREA") return;
  const input = el as HTMLInputElement;
  const current = String(input.value ?? "");
  if (current === "") return;
  setValue(input, current.slice(0, -1));
  el.dispatchEvent(construct("input", "InputEvent", { bubbles: true, inputType: "deleteContentBackward", data: null }));
}

interface Key {
  key: string;
  code: string;
  hold?: boolean;
  release?: boolean;
}

const NAMED_CODES: Record<string, string> = { Shift: "ShiftLeft", Control: "ControlLeft", Alt: "AltLeft", Meta: "MetaLeft", Space: "Space" };

function codeOf(key: string): string {
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (key === " ") return "Space";
  return NAMED_CODES[key] ?? key;
}

function keyOfCode(code: string): string {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (code === "Space") return " ";
  const modifier = /^(Shift|Control|Alt|Meta)(Left|Right)$/.exec(code);
  return modifier ? modifier[1] : code;
}

/** Key descriptors: a character is itself, `{Enter}` a key by name, `[KeyA]` a key by code, `{Shift>}` holds a key until `{/Shift}` and `{{` or `[[` a literal bracket. */
function parseKeys(text: string): Key[] {
  const out: Key[] = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if ((c === "{" || c === "[") && text[i + 1] === c) {
      out.push({ key: c, code: "" });
      i += 2;
      continue;
    }
    if (c === "{" || c === "[") {
      const close = text.indexOf(c === "{" ? "}" : "]", i);
      if (close === -1) throw new Error(`keyboard: an unclosed ${c} in ${JSON.stringify(text)}`);
      let name = text.slice(i + 1, close);
      i = close + 1;
      const release = name.startsWith("/");
      if (release) name = name.slice(1);
      const hold = name.endsWith(">");
      if (hold) name = name.slice(0, -1);
      const key = c === "{" ? (name === "Space" ? " " : name) : keyOfCode(name);
      out.push({ key, code: c === "[" ? name : codeOf(name), hold, release });
      continue;
    }
    out.push({ key: c, code: codeOf(c) });
    i++;
  }
  return out;
}

function clickableBySpace(el: Element): boolean {
  return el.tagName === "BUTTON" || (el.tagName === "INPUT" && ["button", "submit", "reset", "checkbox", "radio", "image"].includes(typeOf(el)));
}

function tabbables(): Element[] {
  const all = Array.from(document.body.querySelectorAll("*")).filter((el) => isFocusable(el) && el.getAttribute("tabindex") !== "-1");
  const order = (el: Element) => Number(el.getAttribute("tabindex") ?? "0");
  return [...all.filter((el) => order(el) > 0).sort((a, b) => order(a) - order(b)), ...all.filter((el) => !(order(el) > 0))];
}

function moveFocus(backwards: boolean): void {
  const list = tabbables();
  if (list.length === 0) return;
  const at = list.indexOf(active());
  const next = at === -1 ? (backwards ? list[list.length - 1] : list[0]) : list[(at + (backwards ? -1 : 1) + list.length) % list.length];
  (next as HTMLElement).focus();
}

function createUser(options: UserOptions = {}): UserEvent {
  const held = { Shift: false, Control: false, Alt: false, Meta: false };
  let hovered: Element | null = null;
  const modifiers = () => ({ shiftKey: held.Shift, ctrlKey: held.Control, altKey: held.Alt, metaKey: held.Meta });
  const mouse = (el: Element, name: string, init: Record<string, unknown> = {}) => el.dispatchEvent(createEvent(name, el, { ...modifiers(), ...init }));

  const hover = (el: Element) => {
    if (hovered === el) return;
    if (hovered && hovered.isConnected) unhover(hovered);
    hovered = el;
    for (const name of ["pointerOver", "pointerEnter", "mouseOver", "mouseEnter", "pointerMove", "mouseMove"]) mouse(el, name);
  };
  const unhover = (el: Element) => {
    for (const name of ["pointerMove", "mouseMove", "pointerOut", "pointerLeave", "mouseOut", "mouseLeave"]) mouse(el, name);
    if (hovered === el) hovered = null;
  };
  const press = (el: Element, count: number, skipHover: boolean) => {
    if (!skipHover) hover(el);
    for (let i = 1; i <= count; i++) {
      mouse(el, "pointerDown");
      const down = isDisabled(el) ? false : mouse(el, "mouseDown", { detail: i });
      if (down) focusOn(el);
      mouse(el, "pointerUp");
      if (isDisabled(el)) continue;
      mouse(el, "mouseUp", { detail: i });
      click(el, { ...modifiers(), detail: i });
      if (i === 2) mouse(el, "dblClick", { detail: 2 });
    }
  };

  const keydownDefault = (target: Element, k: Key) => {
    if (k.key.length === 1 && !held.Control && !held.Meta) {
      const pressed = target.dispatchEvent(construct("keypress", "KeyboardEvent", { key: k.key, code: k.code, bubbles: true, cancelable: true, charCode: k.key.charCodeAt(0), ...modifiers() }));
      if (pressed && isEditable(target) && !(k.key === " " && clickableBySpace(target))) insertText(target, k.key);
      return;
    }
    switch (k.key) {
      case "Enter": {
        target.dispatchEvent(construct("keypress", "KeyboardEvent", { key: "Enter", code: "Enter", bubbles: true, cancelable: true, charCode: 13, ...modifiers() }));
        if (target.tagName === "TEXTAREA" && isEditable(target)) {
          insertText(target, "\n");
          return;
        }
        if (target.tagName === "BUTTON" || (target.tagName === "A" && target.hasAttribute("href")) || (target.tagName === "INPUT" && ["button", "submit", "reset", "image"].includes(typeOf(target)))) {
          click(target);
          return;
        }
        if (target.tagName === "INPUT") {
          const form = formOf(target);
          if (!form) return;
          const submitter = form.querySelector("button:not([type]), button[type=submit], input[type=submit], input[type=image]");
          if (submitter && !isDisabled(submitter)) click(submitter);
          else if (Array.from(form.querySelectorAll("input")).filter((i) => TEXT_TYPES.includes(typeOf(i))).length === 1) submitForm(form, null);
        }
        return;
      }
      case "Backspace":
        if (isEditable(target)) deleteBackward(target);
        return;
      case "Tab":
        moveFocus(held.Shift);
        return;
    }
  };

  const keyboard = async (text: string) => {
    for (const k of parseKeys(text)) {
      if (!k.release) {
        if (k.key in held) held[k.key as keyof typeof held] = true;
        const target = active();
        const down = target.dispatchEvent(construct("keydown", "KeyboardEvent", { key: k.key, code: k.code, bubbles: true, cancelable: true, ...modifiers() }));
        if (down) keydownDefault(target, k);
      }
      if (!k.hold) {
        const target = active();
        const up = target.dispatchEvent(construct("keyup", "KeyboardEvent", { key: k.key, code: k.code, bubbles: true, cancelable: true, ...modifiers() }));
        if (up && k.key === " " && clickableBySpace(target)) click(target);
        if (k.key in held) held[k.key as keyof typeof held] = false;
      }
      await settle();
    }
  };

  const choose = async (el: Element, values: string | Element | (string | Element)[], on: boolean) => {
    const wanted = Array.isArray(values) ? values : [values];
    if (isDisabled(el)) throw new Error(`${on ? "selectOptions" : "deselectOptions"}: the element is disabled`);
    if (el.tagName !== "SELECT") {
      for (const value of wanted) {
        const option = typeof value === "string" ? Array.from(el.querySelectorAll("[role=option]")).find((o) => o.textContent?.trim() === value) : value;
        if (!option) throw new Error(`selectOptions: no option ${show(value)}`);
        press(option, 1, options.skipHover ?? false);
      }
      await settle();
      return;
    }
    const select = el as HTMLSelectElement;
    const all = Array.from(select.querySelectorAll("option")) as HTMLOptionElement[];
    const chosen = wanted.map((value) => {
      const option = typeof value === "string" ? all.find((o) => String(o.value) === value || o.textContent === value) : all.find((o) => o === value);
      if (!option) throw new Error(`selectOptions: no option ${show(value)} in the select`);
      return option;
    });
    if (!select.multiple && chosen.length > 1) throw new Error("selectOptions: a select that is not multiple takes one value");
    if (!on && !select.multiple) throw new Error("deselectOptions: only a multiple select can have an option deselected");
    focusOn(select);
    for (const option of chosen) {
      if (on) option.selected = true;
      else {
        const keep = all.filter((o) => o.selected && o !== option);
        option.selected = false;
        for (const o of keep) o.selected = true;
      }
      select.dispatchEvent(createEvent("input", select));
      select.dispatchEvent(createEvent("change", select));
    }
    await settle();
  };

  return {
    async click(el, clickOptions = {}) {
      press(el, 1, clickOptions.skipHover ?? options.skipHover ?? false);
      await settle();
    },
    async dblClick(el) {
      press(el, 2, options.skipHover ?? false);
      await settle();
    },
    async tripleClick(el) {
      press(el, 3, options.skipHover ?? false);
      await settle();
    },
    async hover(el) {
      hover(el);
      await settle();
    },
    async unhover(el) {
      unhover(el);
      await settle();
    },
    async tab(tabOptions = {}) {
      await keyboard(tabOptions.shift ? "{Shift>}{Tab}{/Shift}" : "{Tab}");
    },
    async type(el, text, typeOptions = {}) {
      if (typeOptions.skipClick) focusOn(el);
      else press(el, 1, options.skipHover ?? false);
      await keyboard(text);
    },
    keyboard,
    async clear(el) {
      focusOn(el);
      if (!isEditable(el)) throw new Error(`clear: ${show(el)} is not editable`);
      setValue(el as HTMLElement, "");
      el.dispatchEvent(construct("input", "InputEvent", { bubbles: true, inputType: "deleteContentBackward", data: null }));
      await settle();
    },
    selectOptions: (el, values) => choose(el, values, true),
    deselectOptions: (el, values) => choose(el, values, false),
    async upload(el, files) {
      if (isDisabled(el)) return;
      const list = Array.isArray(files) ? files : [files];
      Object.defineProperty(el, "files", { value: Object.assign([...list], { item: (i: number) => list[i] ?? null }), configurable: true });
      el.dispatchEvent(createEvent("input", el));
      el.dispatchEvent(createEvent("change", el));
      await settle();
    },
    async paste(text) {
      const target = active();
      const event = construct("paste", "ClipboardEvent", { bubbles: true, cancelable: true });
      Object.defineProperty(event, "clipboardData", { value: { getData: () => text, types: ["text/plain"] }, configurable: true });
      if (target.dispatchEvent(event) && isEditable(target)) insertText(target, text);
      await settle();
    },
  };
}

/** A user at the keyboard and the pointer. `userEvent.setup()` gives a session holding its own modifier keys and hover; the same methods are on `userEvent` itself. Every method settles before it resolves. */
export const userEvent: UserEvent & { setup(options?: UserOptions): UserEvent } = Object.assign(createUser(), { setup: (options?: UserOptions) => createUser(options) });
