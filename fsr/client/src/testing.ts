import type { ReactElement } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { registeredIslands } from "./boot.js";
import { decodeValue, encodeValue, SfValue } from "./values.js";

/** What `fsr test` installs before a spec file loads. */
interface Sf {
  ctx(spec: string): number;
  use(id: number): void;
  session(id: number): string;
  calls(id: number): string;
  render(module: string, props: string): string | null;
  idle(): Promise<void>;
  advance(ms: number): Promise<void>;
}

function sf(): Sf {
  const s = (globalThis as { __sf?: Sf }).__sf;
  if (!s) throw new Error("@snapfire/fsr-client/testing runs under `fsr test` only");
  return s;
}

type Method = (args: never) => unknown;

export interface Mock<Input = unknown> {
  session?: Record<string, unknown>;
  services?: Record<string, Record<string, Method>>;
  input?: Input;
  params?: Record<string, string>;
  query?: Record<string, string>;
  identity?: { subject: string; claims?: Record<string, unknown> };
}

export interface ServiceCall {
  service: string;
  method: string;
  args: Record<string, unknown>;
}

/** A request context an action runs under when a rendered page calls it. `session` and `trace` read back after every call. */
export interface TestCtx {
  readonly id: number;
  readonly session: Record<string, unknown>;
  readonly trace: { calls: ServiceCall[] };
}

const mocks = new Map<string, Method>();

export function ctx(mock: Mock = {}): TestCtx {
  const methods: string[] = [];
  for (const [service, table] of Object.entries(mock.services ?? {})) {
    for (const method of Object.keys(table)) methods.push(`${service}.${method}`);
  }
  const spec = {
    session: encodeValue((mock.session ?? {}) as SfValue),
    params: mock.params ?? {},
    query: mock.query ?? {},
    input: mock.input === undefined ? null : encodeValue(mock.input as SfValue),
    identity: mock.identity ?? null,
    methods,
  };
  const id = sf().ctx(JSON.stringify(spec));
  for (const [service, table] of Object.entries(mock.services ?? {})) {
    for (const [method, fn] of Object.entries(table)) mocks.set(`${id}:${service}.${method}`, fn);
  }
  return {
    id,
    get session() {
      return decodeValue(JSON.parse(sf().session(id))) as Record<string, unknown>;
    },
    get trace() {
      return { calls: decodeValue(JSON.parse(sf().calls(id))) as unknown as ServiceCall[] };
    },
  };
}

/** Called by the runner when a body under test reaches a mocked service method. Answers synchronously: a mock is a function of its arguments. */
function callMock(key: string, args: string): string {
  const fn = mocks.get(key);
  if (!fn) throw new Error(`no mock for ${key}`);
  const result = fn(decodeValue(JSON.parse(args)) as never);
  if (result !== null && typeof result === "object" && typeof (result as { then?: unknown }).then === "function") {
    throw new Error(`the mock for ${key} returned a promise; a mock answers synchronously`);
  }
  return JSON.stringify(encodeValue(result as SfValue));
}

interface Case {
  name: string;
  body: () => Promise<void> | void;
}

const cases: Case[] = [];

export function test(name: string, body: () => Promise<void> | void): void {
  cases.push({ name, body });
}

Object.assign(globalThis, {
  __sf_call: callMock,
  __sf_tests: () => cases.map((c) => c.name),
  __sf_run: (i: number) => {
    const run = Promise.resolve().then(() => cases[i].body());
    run.catch(() => {});
    return run;
  },
});

export class AssertionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AssertionError";
  }
}

/** Values the way a test reads them: `1n` and `1` stay distinct, strings are quoted. */
export function show(value: unknown): string {
  if (typeof value === "bigint") return `${value}n`;
  if (typeof value === "string") return JSON.stringify(value);
  if (value === null || typeof value !== "object") return String(value);
  if (Array.isArray(value)) return `[${value.map(show).join(", ")}]`;
  if (value instanceof Uint8Array) return `Uint8Array(${value.length})`;
  const entries = Object.entries(value as Record<string, unknown>).map(([k, v]) => `${JSON.stringify(k)}: ${show(v)}`);
  return `{ ${entries.join(", ")} }`;
}

export function equal(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => equal(x, b[i]));
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && equal((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

export const assert = {
  ok(value: unknown, message?: string): void {
    if (!value) throw new AssertionError(message ?? `assert.ok: ${show(value)}`);
  },
  equal(actual: unknown, expected: unknown, message?: string): void {
    if (!equal(actual, expected)) throw new AssertionError(`${message ?? "assert.equal"}\n  actual:   ${show(actual)}\n  expected: ${show(expected)}`);
  },
  throws(run: () => unknown, match?: string | RegExp): void {
    try {
      run();
    } catch (e) {
      matchError(e, match);
      return;
    }
    throw new AssertionError("assert.throws: nothing was thrown");
  },
  async rejects(run: Promise<unknown> | (() => Promise<unknown>), match?: string | RegExp): Promise<void> {
    try {
      await (typeof run === "function" ? run() : run);
    } catch (e) {
      matchError(e, match);
      return;
    }
    throw new AssertionError("assert.rejects: the promise resolved");
  },
};

function matchError(e: unknown, match?: string | RegExp): void {
  if (match === undefined) return;
  const text = e instanceof Error ? `${(e as { kind?: string }).kind ?? ""} ${e.message}` : String(e);
  const hit = typeof match === "string" ? text.includes(match) : match.test(text);
  if (!hit) throw new AssertionError(`expected an error matching ${show(match instanceof RegExp ? String(match) : match)}, got ${show(text.trim())}`);
}

export interface Rendered {
  container: HTMLElement;
  root: Root;
  /** The module id the server rendered and React hydrated over; `null` when the component mounted fresh. */
  hydrated: string | null;
  unmount(): void;
}

/** Runs everything that happens now: microtasks, action calls, their re-renders and timers already due. A timer set for later waits for `advance`. */
export function settle(): Promise<void> {
  return sf().idle();
}

/** Moves the clock `ms` forward and settles, so timers due by then fire in order. Time never passes on its own. */
export function advance(ms: number): Promise<void> {
  return sf().advance(ms);
}

async function moduleOf(type: unknown): Promise<string | null> {
  for (const [id, entry] of registeredIslands()) {
    const mod = await entry.loader();
    if (mod === type) return id;
  }
  return null;
}

/** Mounts `element` under a fresh container. A page the server renders is hydrated over its own markup, so a mismatch fails here the way it would in a browser; anything else mounts fresh. */
export async function render(element: ReactElement, options: { ctx?: TestCtx; hydrate?: boolean } = {}): Promise<Rendered> {
  sf().use(options.ctx?.id ?? 0);
  const container = document.createElement("div");
  document.body.appendChild(container);
  const module = options.hydrate === false ? null : await moduleOf(element.type);
  const html = module === null ? null : sf().render(module, JSON.stringify(encodeValue(element.props as SfValue)));
  let root: Root;
  let hydrated: string | null = null;
  if (html !== null) {
    container.innerHTML = html;
    root = hydrateRoot(container, element);
    hydrated = module;
  } else {
    root = createRoot(container);
    root.render(element);
  }
  await settle();
  return {
    container,
    root,
    hydrated,
    unmount() {
      root.unmount();
      container.remove();
    },
  };
}

type Matcher = string | RegExp;

function normalise(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function ownText(el: Element): string {
  let out = "";
  for (const node of Array.from(el.childNodes)) {
    if (node.nodeType === 3) out += node.textContent ?? "";
  }
  return normalise(out);
}

function matches(text: string, matcher: Matcher): boolean {
  return typeof matcher === "string" ? text === matcher : matcher.test(text);
}

function all(root: ParentNode, pick: (el: HTMLElement) => boolean): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>("*")).filter(pick);
}

function one(what: string, found: HTMLElement[]): HTMLElement {
  if (found.length === 1) return found[0];
  if (found.length === 0) throw new AssertionError(`no element ${what}`);
  throw new AssertionError(`${found.length} elements ${what}: ${found.map((el) => `<${el.tagName.toLowerCase()}>`).join(", ")}`);
}

/** Queries over the document, by the text an element itself holds, its label, its placeholder or its `data-testid`. */
export const screen = {
  getByText(matcher: Matcher, root: ParentNode = document.body): HTMLElement {
    return one(`with text ${show(matcher instanceof RegExp ? String(matcher) : matcher)}`, screen.getAllByText(matcher, root));
  },
  queryByText(matcher: Matcher, root: ParentNode = document.body): HTMLElement | null {
    const found = screen.getAllByText(matcher, root);
    return found.length === 0 ? null : found[0];
  },
  getAllByText(matcher: Matcher, root: ParentNode = document.body): HTMLElement[] {
    return all(root, (el) => matches(ownText(el), matcher));
  },
  getByLabelText(matcher: Matcher, root: ParentNode = document.body): HTMLElement {
    const labelled = all(root, (el) => matches(normalise(el.getAttribute("aria-label") ?? ""), matcher));
    const byLabel = Array.from(root.querySelectorAll<HTMLLabelElement>("label"))
      .filter((label) => matches(ownText(label), matcher) || matches(normalise(label.textContent ?? ""), matcher))
      .flatMap((label) => {
        const target = label.getAttribute("for");
        const control = target ? root.querySelector<HTMLElement>(`#${CSS.escape(target)}`) : label.querySelector<HTMLElement>("input, select, textarea, button");
        return control ? [control] : [];
      });
    return one(`labelled ${show(matcher instanceof RegExp ? String(matcher) : matcher)}`, [...labelled, ...byLabel]);
  },
  getByPlaceholderText(matcher: Matcher, root: ParentNode = document.body): HTMLElement {
    return one(`with placeholder ${show(String(matcher))}`, all(root, (el) => matches(el.getAttribute("placeholder") ?? "", matcher)));
  },
  getByTestId(id: string, root: ParentNode = document.body): HTMLElement {
    return one(`with data-testid ${show(id)}`, all(root, (el) => el.getAttribute("data-testid") === id));
  },
};

/** Sets a form control's value the way a user would, past React's own value tracking, so the `input` event that follows is seen as a change. */
function setValue(el: HTMLElement, value: string): void {
  if (el.tagName === "SELECT") {
    const option = Array.from(el.querySelectorAll("option")).find((o) => String(o.value) === value);
    if (!option) throw new AssertionError(`fireEvent.change: no option with value ${show(value)}`);
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

/** Dispatches DOM events and settles the engine after each, so the assertion that follows sees the re-render. */
export const fireEvent = {
  click(el: Element): Promise<void> {
    el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    return settle();
  },
  change(el: Element, value: string): Promise<void> {
    setValue(el as HTMLElement, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return settle();
  },
  submit(el: Element): Promise<void> {
    const form = el instanceof HTMLFormElement ? el : el.closest("form");
    if (!form) throw new AssertionError("fireEvent.submit: no form");
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    return settle();
  },
  keyDown(el: Element, key: string): Promise<void> {
    el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    return settle();
  },
};
