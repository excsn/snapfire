import { AssertionError, show } from "./harness.js";
import { accessibleDescription, accessibleName, isInaccessible, rolesOf } from "./queries.js";

/** The mark an asymmetric matcher carries. */
const ASYMMETRIC = Symbol.for("sf.asymmetricMatcher");

/** A value that decides equality itself: what `expect.any(Number)` and the other `expect.*` helpers build. */
export interface AsymmetricMatcher {
  $$typeof: symbol;
  asymmetricMatch(other: unknown): boolean;
  toString(): string;
}

function isAsymmetric(value: unknown): value is AsymmetricMatcher {
  return typeof value === "object" && value !== null && (value as { $$typeof?: unknown }).$$typeof === ASYMMETRIC;
}

function asymmetric(text: string, match: (other: unknown) => boolean): AsymmetricMatcher {
  return { $$typeof: ASYMMETRIC, asymmetricMatch: match, toString: () => text };
}

/** Whether one side is a bigint and the other a whole number saying the same thing: an integer field reads back as a bigint and a test writes the number it stands for. */
function sameInteger(a: unknown, b: unknown): boolean {
  const [big, num] = typeof a === "bigint" ? [a, b] : [b, a];
  return typeof big === "bigint" && typeof num === "number" && Number.isInteger(num) && BigInt(num) === big;
}

function isNode(value: unknown): value is Node {
  return typeof value === "object" && value !== null && typeof (value as { nodeType?: unknown }).nodeType === "number";
}

/** `toEqual`'s equality: structural, a property holding `undefined` counted as absent, a bigint equal to the whole number it stands for. `strict` is `toStrictEqual`'s: `undefined` properties count, prototypes must match and a bigint is not a number. */
export function equals(a: unknown, b: unknown, strict = false, seen: [unknown, unknown][] = []): boolean {
  if (isAsymmetric(b)) return b.asymmetricMatch(a);
  if (isAsymmetric(a)) return a.asymmetricMatch(b);
  if (Object.is(a, b)) return true;
  if (!strict && sameInteger(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (seen.some(([x, y]) => x === a && y === b)) return true;
  seen = [...seen, [a, b]];
  if (isNode(a) || isNode(b)) return isNode(a) && isNode(b) && (a === b || (typeof (a as { isEqualNode?: unknown }).isEqualNode === "function" && (a as Node).isEqualNode(b as Node)));
  if (a instanceof Date || b instanceof Date) return a instanceof Date && b instanceof Date && a.getTime() === b.getTime();
  if (a instanceof RegExp || b instanceof RegExp) return String(a) === String(b);
  if (strict && Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (a instanceof Map || b instanceof Map) {
    if (!(a instanceof Map && b instanceof Map) || a.size !== b.size) return false;
    return Array.from(a.entries()).every(([k, v]) => b.has(k) && equals(v, b.get(k), strict, seen));
  }
  if (a instanceof Set || b instanceof Set) {
    if (!(a instanceof Set && b instanceof Set) || a.size !== b.size) return false;
    const rest = Array.from(b.values());
    return Array.from(a.values()).every((v) => rest.some((w) => equals(v, w, strict, seen)));
  }
  if (ArrayBuffer.isView(a) || ArrayBuffer.isView(b)) {
    if (!(ArrayBuffer.isView(a) && ArrayBuffer.isView(b))) return false;
    const x = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
    const y = new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
    return x.length === y.length && x.every((byte, i) => byte === y[i]);
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (strict && (i in a) !== (i in b)) return false;
      if (!equals(a[i], b[i], strict, seen)) return false;
    }
    return true;
  }
  const keys = (o: object) => Object.keys(o).filter((k) => strict || (o as Record<string, unknown>)[k] !== undefined);
  const ka = keys(a);
  const kb = keys(b);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && equals((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], strict, seen));
}

/** `toMatchObject`'s subset: every property `expected` names is in `actual` and matches, recursively; an array matches only an array of the same length. */
function matchesObject(actual: unknown, expected: unknown): boolean {
  if (isAsymmetric(expected)) return expected.asymmetricMatch(actual);
  if (typeof expected !== "object" || expected === null || expected instanceof Date || expected instanceof RegExp) return equals(actual, expected);
  if (typeof actual !== "object" || actual === null) return false;
  if (Array.isArray(expected)) return Array.isArray(actual) && actual.length === expected.length && expected.every((e, i) => matchesObject(actual[i], e));
  return Object.keys(expected).every((k) => k in (actual as object) && matchesObject((actual as Record<string, unknown>)[k], (expected as Record<string, unknown>)[k]));
}

function typeName(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}

// ---- mock functions ----

/** A promise a mock function answered with through `mockResolvedValue` or `mockRejectedValue`, marked with how it settles, so a service mock can answer synchronously all the same. */
export const SETTLED: unique symbol = Symbol.for("sf.settled") as never;

interface Settled {
  ok: boolean;
  value: unknown;
}

function marked<T>(promise: Promise<T>, settled: Settled): Promise<T> {
  Object.defineProperty(promise, SETTLED, { value: settled });
  return promise;
}

export interface MockResult {
  type: "return" | "throw" | "incomplete";
  value: unknown;
}

export interface MockState<A extends unknown[], R> {
  calls: A[];
  results: MockResult[];
  instances: unknown[];
  contexts: unknown[];
  invocationCallOrder: number[];
  readonly lastCall: A | undefined;
}

/** A function that records its calls, what `fn()` returns. */
export interface MockInstance<A extends unknown[] = any[], R = any> {
  (...args: A): R;
  new (...args: A): R;
  mock: MockState<A, R>;
  _isMockFunction: true;
  getMockName(): string;
  mockName(name: string): this;
  getMockImplementation(): ((...args: A) => R) | undefined;
  mockImplementation(impl: (...args: A) => R): this;
  mockImplementationOnce(impl: (...args: A) => R): this;
  mockReturnValue(value: R): this;
  mockReturnValueOnce(value: R): this;
  mockResolvedValue(value: Awaited<R>): this;
  mockResolvedValueOnce(value: Awaited<R>): this;
  mockRejectedValue(error: unknown): this;
  mockRejectedValueOnce(error: unknown): this;
  mockReturnThis(): this;
  mockClear(): this;
  mockReset(): this;
  mockRestore(): void;
}

const mocks = new Set<MockInstance>();
let callOrder = 0;

function resolved(value: unknown): Promise<unknown> {
  return marked(Promise.resolve(value), { ok: true, value });
}

function rejected(error: unknown): Promise<unknown> {
  const promise = Promise.reject(error);
  promise.catch(() => {});
  return marked(promise, { ok: false, value: error });
}

/** A mock function, calling `impl` when given. */
export function fn<A extends unknown[] = any[], R = any>(impl?: (...args: A) => R): MockInstance<A, R> {
  let implementation: ((...args: A) => R) | undefined = impl;
  let once: ((...args: A) => R)[] = [];
  let name = "fn()";
  const state: MockState<A, R> = {
    calls: [],
    results: [],
    instances: [],
    contexts: [],
    invocationCallOrder: [],
    get lastCall() {
      return this.calls[this.calls.length - 1];
    },
  };
  const mock = function (this: unknown, ...args: A): R {
    state.calls.push(args);
    state.instances.push(this);
    state.contexts.push(this);
    state.invocationCallOrder.push(++callOrder);
    const result: MockResult = { type: "incomplete", value: undefined };
    state.results.push(result);
    const run = once.length > 0 ? once.shift() : implementation;
    try {
      const value = run ? run.apply(this, args) : undefined;
      result.type = "return";
      result.value = value;
      return value as R;
    } catch (e) {
      result.type = "throw";
      result.value = e;
      throw e;
    }
  } as unknown as MockInstance<A, R>;
  const clear = () => {
    state.calls.length = 0;
    state.results.length = 0;
    state.instances.length = 0;
    state.contexts.length = 0;
    state.invocationCallOrder.length = 0;
  };
  Object.assign(mock, {
    _isMockFunction: true,
    mock: state,
    getMockName: () => name,
    mockName(next: string) {
      name = next;
      return mock;
    },
    getMockImplementation: () => implementation,
    mockImplementation(next: (...args: A) => R) {
      implementation = next;
      return mock;
    },
    mockImplementationOnce(next: (...args: A) => R) {
      once.push(next);
      return mock;
    },
    mockReturnValue(value: R) {
      implementation = () => value;
      return mock;
    },
    mockReturnValueOnce(value: R) {
      once.push(() => value);
      return mock;
    },
    mockResolvedValue(value: unknown) {
      implementation = () => resolved(value) as R;
      return mock;
    },
    mockResolvedValueOnce(value: unknown) {
      once.push(() => resolved(value) as R);
      return mock;
    },
    mockRejectedValue(error: unknown) {
      implementation = () => rejected(error) as R;
      return mock;
    },
    mockRejectedValueOnce(error: unknown) {
      once.push(() => rejected(error) as R);
      return mock;
    },
    mockReturnThis() {
      implementation = function (this: unknown) {
        return this as R;
      };
      return mock;
    },
    mockClear() {
      clear();
      return mock;
    },
    mockReset() {
      clear();
      implementation = undefined;
      once = [];
      return mock;
    },
    mockRestore() {
      clear();
      implementation = impl;
      once = [];
    },
  });
  mocks.add(mock as MockInstance);
  return mock;
}

export function isMockFunction(value: unknown): value is MockInstance {
  return typeof value === "function" && (value as { _isMockFunction?: unknown })._isMockFunction === true;
}

/** Replaces `object[key]` with a mock function that calls the original until told otherwise; `mockRestore` puts the original back. `access` spies on a getter or a setter instead. */
export function spyOn<T extends object, K extends keyof T>(object: T, key: K, access?: "get" | "set"): MockInstance {
  if (access) {
    const descriptor = Object.getOwnPropertyDescriptor(object, key) ?? Object.getOwnPropertyDescriptor(Object.getPrototypeOf(object), key);
    const original = descriptor?.[access];
    if (typeof original !== "function") throw new TypeError(`spyOn: ${String(key)} has no ${access}ter`);
    const spy = fn(function (this: unknown, ...args: unknown[]) {
      return (original as (...a: unknown[]) => unknown).apply(this, args);
    });
    Object.defineProperty(object, key, { ...descriptor, [access]: spy, configurable: true });
    spy.mockRestore = () => {
      spy.mockClear();
      Object.defineProperty(object, key, descriptor as PropertyDescriptor);
    };
    return spy;
  }
  const original = object[key];
  if (typeof original !== "function") throw new TypeError(`spyOn: ${String(key)} is not a function`);
  const spy = fn(function (this: unknown, ...args: unknown[]) {
    return (original as (...a: unknown[]) => unknown).apply(this, args);
  });
  (object as Record<PropertyKey, unknown>)[key as PropertyKey] = spy;
  spy.mockRestore = () => {
    spy.mockClear();
    (object as Record<PropertyKey, unknown>)[key as PropertyKey] = original;
  };
  return spy;
}

export function clearAllMocks(): void {
  for (const mock of mocks) mock.mockClear();
}

export function resetAllMocks(): void {
  for (const mock of mocks) mock.mockReset();
}

export function restoreAllMocks(): void {
  for (const mock of mocks) mock.mockRestore();
}

// ---- expect ----

export interface MatcherResult {
  pass: boolean;
  message: () => string;
}

export interface MatcherState {
  isNot: boolean;
  promise: "" | "resolves" | "rejects";
  equals(a: unknown, b: unknown): boolean;
  utils: { stringify(value: unknown): string; printReceived(value: unknown): string; printExpected(value: unknown): string };
}

export type MatcherFunction = (this: MatcherState, received: any, ...expected: any[]) => MatcherResult | Promise<MatcherResult>;

const utils = { stringify: show, printReceived: show, printExpected: show };

/** What a failed expectation says: what was expected against what arrived, or, under `.not`, what arrived that should not have. */
function result(pass: boolean, expected: string, received: unknown, extra = ""): MatcherResult {
  return {
    pass,
    message: () => (pass ? `Expected: not ${expected}\nReceived: ${show(received)}` : `Expected: ${expected}\nReceived: ${show(received)}`) + (extra ? `\n\n${extra}` : ""),
  };
}

function requireMock(received: unknown, matcher: string): MockInstance {
  if (!isMockFunction(received)) throw new AssertionError(`${matcher}: the received value must be a mock function, got ${show(received)}`);
  return received;
}

function requireElement(received: unknown, matcher: string): Element {
  if (!isNode(received) || received.nodeType !== 1) throw new AssertionError(`${matcher}: the received value must be an element, got ${show(received)}`);
  return received as Element;
}

function normaliseText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function textMatches(text: string, expected: string | RegExp): boolean {
  if (expected instanceof RegExp) {
    expected.lastIndex = 0;
    return expected.test(text);
  }
  return text.includes(expected);
}

/** The thrown value's text the way `toThrow` reads it: an action failure's kind before its message, so `toThrow("invalid")` matches the kind a body failed with. */
function thrownText(error: unknown): string {
  if (error instanceof Error) return `${(error as { kind?: string }).kind ?? ""} ${error.message}`.trim();
  if (typeof error === "object" && error !== null && "message" in error) return String((error as { message: unknown }).message);
  return String(error);
}

function thrownMatches(error: unknown, expected: unknown): boolean {
  if (expected === undefined) return true;
  if (typeof expected === "string") return thrownText(error).includes(expected);
  if (expected instanceof RegExp) {
    expected.lastIndex = 0;
    return expected.test(thrownText(error));
  }
  if (typeof expected === "function") return error instanceof (expected as new (...a: unknown[]) => unknown);
  if (isAsymmetric(expected)) return expected.asymmetricMatch(error);
  if (expected instanceof Error) return error instanceof Error && error.message === expected.message;
  if (typeof expected === "object" && expected !== null) return matchesObject(error, expected);
  return false;
}

function disabled(el: Element): boolean {
  const controls = ["BUTTON", "INPUT", "SELECT", "TEXTAREA", "OPTGROUP", "OPTION", "FIELDSET"];
  if (controls.includes(el.tagName) && el.hasAttribute("disabled")) return true;
  for (let up = el.parentElement; up; up = up.parentElement) {
    if (up.tagName === "FIELDSET" && up.hasAttribute("disabled") && controls.includes(el.tagName)) {
      const legend = up.querySelector(":scope > legend");
      if (!legend || !legend.contains(el)) return true;
    }
    if (up.tagName === "OPTGROUP" && up.hasAttribute("disabled") && el.tagName === "OPTION") return true;
  }
  return false;
}

function inlineStyle(el: Element): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of (el.getAttribute("style") ?? "").split(";")) {
    const at = part.indexOf(":");
    if (at === -1) continue;
    out[part.slice(0, at).trim().toLowerCase()] = part.slice(at + 1).trim();
  }
  return out;
}

function kebab(name: string): string {
  return name.startsWith("--") ? name : name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

function visible(el: Element): boolean {
  if (!el.isConnected) return false;
  for (let at: Element | null = el; at; at = at.parentElement) {
    if (at.hasAttribute("hidden")) return false;
    const style = inlineStyle(at);
    if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse" || style.opacity === "0") return false;
    const details = at.parentElement;
    if (details?.tagName === "DETAILS" && !details.hasAttribute("open") && at.tagName !== "SUMMARY") return false;
  }
  return true;
}

function valueOf(el: Element): unknown {
  if (el.tagName === "SELECT") {
    const select = el as HTMLSelectElement;
    const chosen = Array.from(select.querySelectorAll("option")).filter((o) => (o as HTMLOptionElement).selected).map((o) => String((o as HTMLOptionElement).value));
    return select.multiple ? chosen : (chosen[0] ?? null);
  }
  const input = el as HTMLInputElement;
  const value = input.value === undefined ? (el.getAttribute("value") ?? "") : String(input.value);
  const type = (el.getAttribute("type") ?? "").toLowerCase();
  if (type === "number" || type === "range") return value === "" ? null : Number(value);
  return value;
}

function formValues(form: Element): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const controls = Array.from(form.querySelectorAll("input, select, textarea")).filter((c) => c.getAttribute("name"));
  const byName = new Map<string, Element[]>();
  for (const c of controls) {
    const name = c.getAttribute("name") as string;
    byName.set(name, [...(byName.get(name) ?? []), c]);
  }
  for (const [name, group] of byName) {
    const type = (group[0].getAttribute("type") ?? "").toLowerCase();
    if (type === "checkbox") {
      out[name] = group.length === 1 && !group[0].hasAttribute("value") ? (group[0] as HTMLInputElement).checked : group.filter((c) => (c as HTMLInputElement).checked).map((c) => c.getAttribute("value") ?? "on");
    } else if (type === "radio") {
      const on = group.find((c) => (c as HTMLInputElement).checked);
      out[name] = on ? on.getAttribute("value") ?? "on" : undefined;
    } else {
      out[name] = group.length === 1 ? valueOf(group[0]) : group.map(valueOf);
    }
  }
  return out;
}

function checkedState(el: Element): boolean | "mixed" | null {
  const type = (el.getAttribute("type") ?? "").toLowerCase();
  if (el.tagName === "INPUT" && (type === "checkbox" || type === "radio")) return (el as HTMLInputElement).checked;
  const aria = el.getAttribute("aria-checked");
  if (aria === "mixed") return "mixed";
  if (aria === "true" || aria === "false") return aria === "true";
  return null;
}

function callsWith(mock: MockInstance, args: unknown[]): boolean {
  return mock.mock.calls.some((call) => equals(call, args));
}

function returns(mock: MockInstance): unknown[] {
  return mock.mock.results.filter((r) => r.type === "return").map((r) => r.value);
}

const unsupported = (name: string): MatcherFunction =>
  function () {
    throw new AssertionError(`${name}: fsr test keeps no snapshot files; assert on the value itself`);
  };

const matchers: Record<string, MatcherFunction> = {
  toBe(received, expected) {
    const pass = Object.is(received, expected) || sameInteger(received, expected);
    return result(pass, show(expected), received, !pass && equals(received, expected) ? "The values are equal but not the same; toEqual compares by value." : "");
  },
  toEqual(received, expected) {
    return result(equals(received, expected), show(expected), received);
  },
  toStrictEqual(received, expected) {
    return result(equals(received, expected, true), show(expected), received);
  },
  toBeTruthy(received) {
    return result(!!received, "a truthy value", received);
  },
  toBeFalsy(received) {
    return result(!received, "a falsy value", received);
  },
  toBeNull(received) {
    return result(received === null, "null", received);
  },
  toBeUndefined(received) {
    return result(received === undefined, "undefined", received);
  },
  toBeDefined(received) {
    return result(received !== undefined, "a defined value", received);
  },
  toBeNaN(received) {
    return result(Number.isNaN(received), "NaN", received);
  },
  toBeGreaterThan(received, n) {
    return result(received > n, `> ${show(n)}`, received);
  },
  toBeGreaterThanOrEqual(received, n) {
    return result(received >= n, `>= ${show(n)}`, received);
  },
  toBeLessThan(received, n) {
    return result(received < n, `< ${show(n)}`, received);
  },
  toBeLessThanOrEqual(received, n) {
    return result(received <= n, `<= ${show(n)}`, received);
  },
  toBeCloseTo(received, n, digits = 2) {
    const pass = received === n || (Number.isFinite(received) && Number.isFinite(n) && Math.abs(Number(n) - Number(received)) < 10 ** -digits / 2);
    return result(pass, `${show(n)} to ${digits} digits`, received);
  },
  toContain(received, item) {
    let pass = false;
    if (typeof received === "string") pass = received.includes(String(item));
    else if (received !== null && received !== undefined && typeof received[Symbol.iterator] === "function") pass = Array.from(received as Iterable<unknown>).some((x) => x === item || sameInteger(x, item));
    return result(pass, `a value containing ${show(item)}`, received);
  },
  toContainEqual(received, item) {
    const pass = received !== null && received !== undefined && typeof received[Symbol.iterator] === "function" && Array.from(received as Iterable<unknown>).some((x) => equals(x, item));
    return result(pass, `a value containing an item equal to ${show(item)}`, received);
  },
  toHaveLength(received, length) {
    const has = received !== null && received !== undefined ? (received as { length?: unknown }).length : undefined;
    return result(has === length, `length ${show(length)}`, received, `Received length: ${show(has)}`);
  },
  toHaveProperty(received, path, ...value) {
    const keys: PropertyKey[] = Array.isArray(path) ? path : String(path).replace(/\[(\w+)\]/g, ".$1").split(".").filter((k) => k !== "");
    let at: unknown = received;
    let found = true;
    for (const key of keys) {
      if (at === null || at === undefined || !(key in Object(at))) {
        found = false;
        break;
      }
      at = (at as Record<PropertyKey, unknown>)[key];
    }
    const pass = found && (value.length === 0 || equals(at, value[0]));
    return result(pass, value.length === 0 ? `a property at ${show(path)}` : `a property at ${show(path)} equal to ${show(value[0])}`, received, found ? `Value there: ${show(at)}` : "");
  },
  toMatch(received, pattern) {
    const pass = typeof received === "string" && textMatches(received, pattern);
    return result(pass, `a string matching ${show(pattern)}`, received);
  },
  toMatchObject(received, expected) {
    return result(matchesObject(received, expected), `an object matching ${show(expected)}`, received);
  },
  toThrow(received, expected) {
    let thrown: { value: unknown } | null = null;
    if (this.promise === "rejects") thrown = { value: received };
    else {
      if (typeof received !== "function") throw new AssertionError(`toThrow: the received value must be a function, got ${show(received)}`);
      try {
        received();
      } catch (e) {
        thrown = { value: e };
      }
    }
    const pass = thrown !== null && thrownMatches(thrown.value, expected);
    return {
      pass,
      message: () =>
        thrown === null ? "Received function did not throw" : `Expected: ${pass ? "not " : ""}${expected === undefined ? "to throw" : `a throw matching ${show(expected)}`}\nThrown: ${show(thrown.value)}`,
    };
  },
  toBeInstanceOf(received, type) {
    return result(received instanceof type, `an instance of ${(type as { name?: string }).name ?? show(type)}`, received);
  },
  toBeTypeOf(received, type) {
    return result(typeof received === type, `a value of type ${show(type)}`, received, `Received type: ${typeName(received)}`);
  },
  toSatisfy(received, predicate) {
    return result(!!predicate(received), "a value satisfying the predicate", received);
  },
  toBeOneOf(received, options: unknown[]) {
    return result(options.some((o) => equals(received, o)), `one of ${show(options)}`, received);
  },
  toHaveBeenCalled(received) {
    const mock = requireMock(received, "toHaveBeenCalled");
    return result(mock.mock.calls.length > 0, "at least one call", mock, `Calls: ${show(mock.mock.calls)}`);
  },
  toHaveBeenCalledOnce(received) {
    const mock = requireMock(received, "toHaveBeenCalledOnce");
    return result(mock.mock.calls.length === 1, "exactly one call", mock, `Calls: ${show(mock.mock.calls)}`);
  },
  toHaveBeenCalledTimes(received, times) {
    const mock = requireMock(received, "toHaveBeenCalledTimes");
    return result(mock.mock.calls.length === times, `${times} calls`, mock, `Received ${mock.mock.calls.length}: ${show(mock.mock.calls)}`);
  },
  toHaveBeenCalledWith(received, ...args) {
    const mock = requireMock(received, "toHaveBeenCalledWith");
    return result(callsWith(mock, args), `a call with ${show(args)}`, mock, `Calls: ${show(mock.mock.calls)}`);
  },
  toHaveBeenCalledExactlyOnceWith(received, ...args) {
    const mock = requireMock(received, "toHaveBeenCalledExactlyOnceWith");
    return result(mock.mock.calls.length === 1 && callsWith(mock, args), `exactly one call, with ${show(args)}`, mock, `Calls: ${show(mock.mock.calls)}`);
  },
  toHaveBeenLastCalledWith(received, ...args) {
    const mock = requireMock(received, "toHaveBeenLastCalledWith");
    return result(mock.mock.calls.length > 0 && equals(mock.mock.lastCall, args), `a last call with ${show(args)}`, mock, `Last call: ${show(mock.mock.lastCall)}`);
  },
  toHaveBeenNthCalledWith(received, n: number, ...args) {
    const mock = requireMock(received, "toHaveBeenNthCalledWith");
    return result(equals(mock.mock.calls[n - 1], args), `call ${n} with ${show(args)}`, mock, `Call ${n}: ${show(mock.mock.calls[n - 1])}`);
  },
  toHaveReturned(received) {
    const mock = requireMock(received, "toHaveReturned");
    return result(returns(mock).length > 0, "a return", mock);
  },
  toHaveReturnedTimes(received, times) {
    const mock = requireMock(received, "toHaveReturnedTimes");
    return result(returns(mock).length === times, `${times} returns`, mock, `Returns: ${returns(mock).length}`);
  },
  toHaveReturnedWith(received, value) {
    const mock = requireMock(received, "toHaveReturnedWith");
    return result(returns(mock).some((r) => equals(r, value)), `a return of ${show(value)}`, mock, `Returns: ${show(returns(mock))}`);
  },
  toHaveLastReturnedWith(received, value) {
    const mock = requireMock(received, "toHaveLastReturnedWith");
    const last = mock.mock.results[mock.mock.results.length - 1];
    return result(last?.type === "return" && equals(last.value, value), `a last return of ${show(value)}`, mock, `Last result: ${show(last?.value)}`);
  },
  toHaveNthReturnedWith(received, n: number, value) {
    const mock = requireMock(received, "toHaveNthReturnedWith");
    const nth = mock.mock.results[n - 1];
    return result(nth?.type === "return" && equals(nth.value, value), `return ${n} of ${show(value)}`, mock, `Result ${n}: ${show(nth?.value)}`);
  },
  toBeInTheDocument(received) {
    const el = requireElement(received, "toBeInTheDocument");
    return result(el.isConnected && el.ownerDocument === document, "an element in the document", el);
  },
  toHaveTextContent(received, text: string | RegExp, options: { normalizeWhitespace?: boolean } = {}) {
    const el = requireElement(received, "toHaveTextContent");
    const content = options.normalizeWhitespace === false ? (el.textContent ?? "") : normaliseText(el.textContent ?? "");
    return result(textMatches(content, text), `text content matching ${show(text)}`, el, `Text content: ${show(content)}`);
  },
  toHaveAttribute(received, name: string, ...value) {
    const el = requireElement(received, "toHaveAttribute");
    const has = el.hasAttribute(name);
    const pass = has && (value.length === 0 || equals(el.getAttribute(name), value[0]));
    return result(pass, value.length === 0 ? `an attribute ${show(name)}` : `${name}=${show(value[0])}`, el, has ? `${name}=${show(el.getAttribute(name))}` : "");
  },
  toHaveClass(received, ...names) {
    const el = requireElement(received, "toHaveClass");
    const options = typeof names[names.length - 1] === "object" ? (names.pop() as { exact?: boolean }) : {};
    const has = (el.getAttribute("class") ?? "").split(/\s+/).filter(Boolean);
    const want = names.flatMap((n) => String(n).split(/\s+/)).filter(Boolean);
    const pass = want.length === 0 ? has.length > 0 : options.exact ? want.length === has.length && want.every((w) => has.includes(w)) : want.every((w) => has.includes(w));
    return result(pass, want.length === 0 ? "a class" : `class ${show(want.join(" "))}`, el, `Classes: ${show(has.join(" "))}`);
  },
  toBeVisible(received) {
    const el = requireElement(received, "toBeVisible");
    return result(visible(el), "a visible element", el);
  },
  toBeDisabled(received) {
    const el = requireElement(received, "toBeDisabled");
    return result(disabled(el), "a disabled element", el);
  },
  toBeEnabled(received) {
    const el = requireElement(received, "toBeEnabled");
    return result(!disabled(el), "an enabled element", el);
  },
  toBeRequired(received) {
    const el = requireElement(received, "toBeRequired");
    return result(el.hasAttribute("required") || el.getAttribute("aria-required") === "true", "a required element", el);
  },
  toBeInvalid(received) {
    const el = requireElement(received, "toBeInvalid");
    return result(el.getAttribute("aria-invalid") === "true" || el.getAttribute("aria-invalid") === "", "an invalid element", el);
  },
  toBeValid(received) {
    const el = requireElement(received, "toBeValid");
    return result(!(el.getAttribute("aria-invalid") === "true" || el.getAttribute("aria-invalid") === ""), "a valid element", el);
  },
  toBeChecked(received) {
    const el = requireElement(received, "toBeChecked");
    const state = checkedState(el);
    if (state === null) throw new AssertionError(`toBeChecked: ${show(el)} is not a checkbox, a radio or an element with aria-checked`);
    return result(state === true, "a checked element", el);
  },
  toBePartiallyChecked(received) {
    const el = requireElement(received, "toBePartiallyChecked");
    return result(checkedState(el) === "mixed" || (el as HTMLInputElement).indeterminate === true, "a partially checked element", el);
  },
  toHaveValue(received, ...value) {
    const el = requireElement(received, "toHaveValue");
    const type = (el.getAttribute("type") ?? "").toLowerCase();
    if (type === "checkbox" || type === "radio") throw new AssertionError("toHaveValue: a checkbox or radio has no value to check; use toBeChecked");
    const has = valueOf(el);
    const pass = value.length === 0 ? has !== "" && has !== null && !(Array.isArray(has) && has.length === 0) : equals(has, value[0]);
    return result(pass, value.length === 0 ? "a value" : `value ${show(value[0])}`, el, `Value: ${show(has)}`);
  },
  toHaveDisplayValue(received, value: string | RegExp | (string | RegExp)[]) {
    const el = requireElement(received, "toHaveDisplayValue");
    const shown = el.tagName === "SELECT" ? Array.from(el.querySelectorAll("option")).filter((o) => (o as HTMLOptionElement).selected).map((o) => o.textContent ?? "") : [String((el as HTMLInputElement).value ?? "")];
    const want = Array.isArray(value) ? value : [value];
    const pass = want.length === shown.length && want.every((w) => shown.some((s) => (w instanceof RegExp ? textMatches(s, w) : s === w)));
    return result(pass, `display value ${show(value)}`, el, `Display value: ${show(shown)}`);
  },
  toHaveFocus(received) {
    const el = requireElement(received, "toHaveFocus");
    return result(el.ownerDocument.activeElement === el, "the focused element", el, `Focused: ${show(el.ownerDocument.activeElement)}`);
  },
  toBeEmptyDOMElement(received) {
    const el = requireElement(received, "toBeEmptyDOMElement");
    return result(Array.from(el.childNodes).every((n) => n.nodeType === 8), "an empty element", el, `Content: ${show(el.innerHTML)}`);
  },
  toContainElement(received, other: Element | null) {
    const el = requireElement(received, "toContainElement");
    return result(other !== null && el.contains(other), `an element containing ${show(other)}`, el);
  },
  toContainHTML(received, html: string) {
    const el = requireElement(received, "toContainHTML");
    return result(el.outerHTML.includes(html), `markup containing ${show(html)}`, el, `Markup: ${show(el.outerHTML)}`);
  },
  toHaveStyle(received, css: string | Record<string, unknown>) {
    const el = requireElement(received, "toHaveStyle");
    const style = inlineStyle(el);
    const want: [string, string][] =
      typeof css === "string"
        ? css.split(";").map((p) => p.split(":")).filter((p) => p.length >= 2).map(([k, ...v]) => [k.trim().toLowerCase(), v.join(":").trim()])
        : Object.entries(css).map(([k, v]) => [kebab(k), typeof v === "number" && v !== 0 ? `${v}px` : String(v)]);
    return result(want.every(([k, v]) => style[k] === v), `style ${show(css)}`, el, `Inline style: ${show(el.getAttribute("style") ?? "")}`);
  },
  toHaveFormValues(received, expected: Record<string, unknown>) {
    const el = requireElement(received, "toHaveFormValues");
    const values = formValues(el);
    return result(matchesObject(values, expected), `form values ${show(expected)}`, el, `Form values: ${show(values)}`);
  },
  toHaveAccessibleName(received, ...name) {
    const el = requireElement(received, "toHaveAccessibleName");
    const has = accessibleName(el);
    const pass = name.length === 0 ? has !== "" : name[0] instanceof RegExp ? textMatches(has, name[0]) : equals(has, name[0]);
    return result(pass, name.length === 0 ? "an accessible name" : `accessible name ${show(name[0])}`, el, `Accessible name: ${show(has)}`);
  },
  toHaveAccessibleDescription(received, ...description) {
    const el = requireElement(received, "toHaveAccessibleDescription");
    const has = accessibleDescription(el);
    const pass = description.length === 0 ? has !== "" : description[0] instanceof RegExp ? textMatches(has, description[0]) : equals(has, description[0]);
    return result(pass, description.length === 0 ? "an accessible description" : `accessible description ${show(description[0])}`, el, `Accessible description: ${show(has)}`);
  },
  toHaveRole(received, role: string) {
    const el = requireElement(received, "toHaveRole");
    const roles = isInaccessible(el) ? [] : rolesOf(el);
    return result(roles.includes(role), `role ${show(role)}`, el, `Roles: ${show(roles)}`);
  },
  toMatchSnapshot: unsupported("toMatchSnapshot"),
  toMatchInlineSnapshot: unsupported("toMatchInlineSnapshot"),
  toThrowErrorMatchingSnapshot: unsupported("toThrowErrorMatchingSnapshot"),
  toThrowErrorMatchingInlineSnapshot: unsupported("toThrowErrorMatchingInlineSnapshot"),
};

const aliases: Record<string, string> = {
  toThrowError: "toThrow",
  toBeCalled: "toHaveBeenCalled",
  toBeCalledTimes: "toHaveBeenCalledTimes",
  toBeCalledWith: "toHaveBeenCalledWith",
  lastCalledWith: "toHaveBeenLastCalledWith",
  nthCalledWith: "toHaveBeenNthCalledWith",
  toReturn: "toHaveReturned",
  toReturnTimes: "toHaveReturnedTimes",
  toReturnWith: "toHaveReturnedWith",
  lastReturnedWith: "toHaveLastReturnedWith",
  nthReturnedWith: "toHaveNthReturnedWith",
};
for (const [alias, name] of Object.entries(aliases)) matchers[alias] = matchers[name];

// ---- assertion counting ----

let counted = 0;
let expectedCount: number | null = null;
let expectedAny = false;

/** Called by the runner before each test. */
export function resetAssertions(): void {
  counted = 0;
  expectedCount = null;
  expectedAny = false;
}

/** Called by the runner after each test body: what `expect.assertions` and `expect.hasAssertions` asked for. */
export function verifyAssertions(): void {
  if (expectedCount !== null && counted !== expectedCount) throw new AssertionError(`expect.assertions(${expectedCount}): ${counted} assertion${counted === 1 ? " was" : "s were"} made`);
  if (expectedAny && counted === 0) throw new AssertionError("expect.hasAssertions(): no assertion was made");
}

// ---- the expect function ----

/** Every matcher, each returning `R`: void for a plain expectation, a promise under `.resolves` and `.rejects`. */
export interface Matchers<R = void> {
  toBe(expected: unknown): R;
  toEqual(expected: unknown): R;
  toStrictEqual(expected: unknown): R;
  toBeTruthy(): R;
  toBeFalsy(): R;
  toBeNull(): R;
  toBeUndefined(): R;
  toBeDefined(): R;
  toBeNaN(): R;
  toBeGreaterThan(n: number | bigint): R;
  toBeGreaterThanOrEqual(n: number | bigint): R;
  toBeLessThan(n: number | bigint): R;
  toBeLessThanOrEqual(n: number | bigint): R;
  toBeCloseTo(n: number, digits?: number): R;
  toContain(item: unknown): R;
  toContainEqual(item: unknown): R;
  toHaveLength(length: number): R;
  toHaveProperty(path: string | PropertyKey[], value?: unknown): R;
  toMatch(pattern: string | RegExp): R;
  toMatchObject(expected: object): R;
  toThrow(expected?: unknown): R;
  toThrowError(expected?: unknown): R;
  toBeInstanceOf(type: unknown): R;
  toBeTypeOf(type: "string" | "number" | "bigint" | "boolean" | "symbol" | "undefined" | "object" | "function"): R;
  toSatisfy(predicate: (value: any) => boolean): R;
  toBeOneOf(options: unknown[]): R;
  toHaveBeenCalled(): R;
  toHaveBeenCalledOnce(): R;
  toHaveBeenCalledTimes(times: number): R;
  toHaveBeenCalledWith(...args: unknown[]): R;
  toHaveBeenCalledExactlyOnceWith(...args: unknown[]): R;
  toHaveBeenLastCalledWith(...args: unknown[]): R;
  toHaveBeenNthCalledWith(n: number, ...args: unknown[]): R;
  toHaveReturned(): R;
  toHaveReturnedTimes(times: number): R;
  toHaveReturnedWith(value: unknown): R;
  toHaveLastReturnedWith(value: unknown): R;
  toHaveNthReturnedWith(n: number, value: unknown): R;
  toBeCalled(): R;
  toBeCalledTimes(times: number): R;
  toBeCalledWith(...args: unknown[]): R;
  lastCalledWith(...args: unknown[]): R;
  nthCalledWith(n: number, ...args: unknown[]): R;
  toReturn(): R;
  toReturnTimes(times: number): R;
  toReturnWith(value: unknown): R;
  lastReturnedWith(value: unknown): R;
  nthReturnedWith(n: number, value: unknown): R;
  toBeInTheDocument(): R;
  toHaveTextContent(text: string | RegExp, options?: { normalizeWhitespace?: boolean }): R;
  toHaveAttribute(name: string, value?: unknown): R;
  toHaveClass(...names: (string | { exact?: boolean })[]): R;
  toBeVisible(): R;
  toBeDisabled(): R;
  toBeEnabled(): R;
  toBeRequired(): R;
  toBeInvalid(): R;
  toBeValid(): R;
  toBeChecked(): R;
  toBePartiallyChecked(): R;
  toHaveValue(value?: unknown): R;
  toHaveDisplayValue(value: string | RegExp | (string | RegExp)[]): R;
  toHaveFocus(): R;
  toBeEmptyDOMElement(): R;
  toContainElement(element: Element | null): R;
  toContainHTML(html: string): R;
  toHaveStyle(css: string | Record<string, unknown>): R;
  toHaveFormValues(values: Record<string, unknown>): R;
  toHaveAccessibleName(name?: string | RegExp): R;
  toHaveAccessibleDescription(description?: string | RegExp): R;
  toHaveRole(role: string): R;
  toMatchSnapshot(): R;
  toMatchInlineSnapshot(snapshot?: string): R;
  toThrowErrorMatchingSnapshot(): R;
  toThrowErrorMatchingInlineSnapshot(snapshot?: string): R;
}

export interface Assertion extends Matchers<void> {
  not: Matchers<void>;
  resolves: Matchers<Promise<void>> & { not: Matchers<Promise<void>> };
  rejects: Matchers<Promise<void>> & { not: Matchers<Promise<void>> };
}

function check(name: string, matcher: MatcherFunction, received: unknown, args: unknown[], isNot: boolean, promise: MatcherState["promise"], message: string | undefined): void | Promise<void> {
  counted++;
  const state: MatcherState = { isNot, promise, equals: (a, b) => equals(a, b), utils };
  const header = `expect(received)${promise ? `.${promise}` : ""}${isNot ? ".not" : ""}.${name}(${args.length > 0 ? "expected" : ""})`;
  const judge = (outcome: MatcherResult): void => {
    if (outcome.pass === isNot) throw new AssertionError(`${message ? `${message}\n\n` : ""}${header}\n\n${outcome.message()}`);
  };
  const outcome = matcher.call(state, received, ...args);
  if (outcome instanceof Promise) return outcome.then(judge);
  judge(outcome);
}

function plain(received: unknown, isNot: boolean, message: string | undefined): Matchers<void> {
  const out: Record<string, (...args: unknown[]) => void | Promise<void>> = {};
  for (const [name, matcher] of Object.entries(matchers)) out[name] = (...args) => check(name, matcher, received, args, isNot, "", message);
  return out as unknown as Matchers<void>;
}

function settling(received: unknown, how: "resolves" | "rejects", isNot: boolean, message: string | undefined): Matchers<Promise<void>> {
  const out: Record<string, (...args: unknown[]) => Promise<void>> = {};
  for (const [name, matcher] of Object.entries(matchers)) {
    out[name] = async (...args) => {
      const pending = typeof received === "function" ? (received as () => unknown)() : received;
      let outcome: { ok: boolean; value: unknown };
      try {
        outcome = { ok: true, value: await pending };
      } catch (error) {
        outcome = { ok: false, value: error };
      }
      const lead = message ? `${message}\n\n` : "";
      if (how === "resolves" && !outcome.ok) throw new AssertionError(`${lead}expect(received).resolves.${name}()\n\nThe promise rejected instead of resolving: ${show(outcome.value)}`);
      if (how === "rejects" && outcome.ok) throw new AssertionError(`${lead}expect(received).rejects.${name}()\n\nThe promise resolved instead of rejecting: ${show(outcome.value)}`);
      await check(name, matcher, outcome.value, args, isNot, how, message);
    };
  }
  return out as unknown as Matchers<Promise<void>>;
}

export interface Expect {
  /** `message` leads the report when the expectation fails. */
  (received: unknown, message?: string): Assertion;
  any(type: unknown): AsymmetricMatcher;
  anything(): AsymmetricMatcher;
  objectContaining(expected: object): AsymmetricMatcher;
  arrayContaining(expected: unknown[]): AsymmetricMatcher;
  stringContaining(expected: string): AsymmetricMatcher;
  stringMatching(expected: string | RegExp): AsymmetricMatcher;
  closeTo(n: number, digits?: number): AsymmetricMatcher;
  not: {
    objectContaining(expected: object): AsymmetricMatcher;
    arrayContaining(expected: unknown[]): AsymmetricMatcher;
    stringContaining(expected: string): AsymmetricMatcher;
    stringMatching(expected: string | RegExp): AsymmetricMatcher;
  };
  assertions(count: number): void;
  hasAssertions(): void;
  extend(more: Record<string, MatcherFunction>): void;
  unreachable(message?: string): never;
}

function anyOf(type: unknown): AsymmetricMatcher {
  const name = (type as { name?: string }).name ?? String(type);
  return asymmetric(`Any<${name}>`, (other) => {
    if (type === String) return typeof other === "string" || other instanceof String;
    if (type === Number) return typeof other === "number" || other instanceof Number;
    if (type === Boolean) return typeof other === "boolean" || other instanceof Boolean;
    if (type === BigInt) return typeof other === "bigint";
    if (type === Symbol) return typeof other === "symbol";
    if (type === Function) return typeof other === "function";
    if (type === Object) return typeof other === "object" && other !== null;
    return typeof type === "function" && other instanceof (type as new (...a: unknown[]) => unknown);
  });
}

const objectContaining = (expected: object) => asymmetric(`ObjectContaining ${show(expected)}`, (other) => typeof other === "object" && other !== null && Object.entries(expected).every(([k, v]) => k in other && equals((other as Record<string, unknown>)[k], v)));
const arrayContaining = (expected: unknown[]) => asymmetric(`ArrayContaining ${show(expected)}`, (other) => Array.isArray(other) && expected.every((e) => other.some((o) => equals(o, e))));
const stringContaining = (expected: string) => asymmetric(`StringContaining ${show(expected)}`, (other) => typeof other === "string" && other.includes(expected));
const stringMatching = (expected: string | RegExp) => asymmetric(`StringMatching ${show(expected)}`, (other) => typeof other === "string" && (typeof expected === "string" ? new RegExp(expected) : expected).test(other));
const negate = (m: AsymmetricMatcher) => asymmetric(`Not${m.toString()}`, (other) => !m.asymmetricMatch(other));

export const expect: Expect = Object.assign(
  (received: unknown, message?: string): Assertion => {
    const assertion = plain(received, false, message) as Assertion;
    assertion.not = plain(received, true, message);
    assertion.resolves = Object.assign(settling(received, "resolves", false, message), { not: settling(received, "resolves", true, message) });
    assertion.rejects = Object.assign(settling(received, "rejects", false, message), { not: settling(received, "rejects", true, message) });
    return assertion;
  },
  {
    any: anyOf,
    anything: () => asymmetric("Anything", (other) => other !== null && other !== undefined),
    objectContaining,
    arrayContaining,
    stringContaining,
    stringMatching,
    closeTo: (n: number, digits = 2) => asymmetric(`NumberCloseTo ${n} (${digits} digits)`, (other) => typeof other === "number" && Math.abs(other - n) < 10 ** -digits / 2),
    not: {
      objectContaining: (e: object) => negate(objectContaining(e)),
      arrayContaining: (e: unknown[]) => negate(arrayContaining(e)),
      stringContaining: (e: string) => negate(stringContaining(e)),
      stringMatching: (e: string | RegExp) => negate(stringMatching(e)),
    },
    assertions(count: number) {
      expectedCount = count;
    },
    hasAssertions() {
      expectedAny = true;
    },
    extend(more: Record<string, MatcherFunction>) {
      Object.assign(matchers, more);
    },
    unreachable(message?: string): never {
      throw new AssertionError(message ?? "expect.unreachable: this line should not be reached");
    },
  },
);
