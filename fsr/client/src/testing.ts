import type { ComponentType, ReactElement, ReactNode } from "react";
import type { Root } from "react-dom/client";

import { boot, discard, registeredIslands } from "./boot.js";
import { advance, AssertionError, settle, sf, show } from "./harness.js";
import { clearAllMocks, fn, isMockFunction, resetAllMocks, resetAssertions, restoreAllMocks, SETTLED, spyOn, verifyAssertions } from "./expect.js";
import { setLocale } from "./locale.js";
import { applyHead, clearRouterCache, enableNavigation } from "./navigator.js";
import { prettyDOM, waitFor, within, type BoundQueries, type WaitForOptions } from "./queries.js";
import type { Hoisted } from "./react.js";
import { reset, seed } from "./store.js";
import { decodeValue, encodeValue, SfValue } from "./values.js";

export { f64 } from "./values.js";
export type { DoubleValue } from "./values.js";
export { advance, AssertionError, settle, show } from "./harness.js";
export { clearAllMocks, expect, fn, isMockFunction, resetAllMocks, restoreAllMocks, spyOn } from "./expect.js";
export type { Assertion, AsymmetricMatcher, Expect, MatcherFunction, MatcherResult, MatcherState, Matchers, MockInstance, MockResult, MockState } from "./expect.js";
export { configure, getDefaultNormalizer, logRoles, prettyDOM, screen, TestingLibraryElementError, waitFor, waitForElementToBeRemoved, within } from "./queries.js";
export type { BoundQueries, ByRoleOptions, Matcher, MatcherOptions, Screen, SelectorMatcherOptions, WaitForOptions } from "./queries.js";
export { createEvent, fireEvent, userEvent } from "./events.js";
export type { EventInit, FireEvent, UserEvent, UserOptions } from "./events.js";

type Method = (args: never) => unknown;

export interface Mock<Input = unknown> {
  session?: Record<string, unknown>;
  services?: Record<string, Record<string, Method>>;
  /** The application's own Rust as `ctx.native` sees it. A spec cannot link the crate, so each module is answered by a function here. */
  native?: Record<string, Record<string, Method>>;
  input?: Input;
  params?: Record<string, string>;
  query?: Record<string, string>;
  identity?: { subject: string; claims?: Record<string, unknown> };
  /** The request's locale, as the configuration spells it; the host's default when absent. */
  locale?: string;
  /** The path the request matched, which a layout or a slot reads to build a link that keeps the page beside it. */
  path?: string;
}

export interface ServiceCall {
  service: string;
  method: string;
  args: Record<string, unknown>;
}

/** A request context an action runs under when a rendered page calls it. `session` and `trace` read back after every call. */
export interface TestCtx {
  readonly id: number;
  readonly locale: string;
  readonly session: Record<string, unknown>;
  readonly trace: { calls: ServiceCall[] };
}

const mocks = new Map<string, Method>();

export function ctx(mock: Mock = {}): TestCtx {
  const methods: string[] = [];
  for (const [service, table] of Object.entries(mock.services ?? {})) {
    for (const method of Object.keys(table)) methods.push(`${service}.${method}`);
  }
  const natives: string[] = [];
  for (const [module, table] of Object.entries(mock.native ?? {})) {
    for (const method of Object.keys(table)) natives.push(`${module}.${method}`);
  }
  const spec = {
    session: encodeValue((mock.session ?? {}) as SfValue),
    params: mock.params ?? {},
    query: mock.query ?? {},
    input: mock.input === undefined ? null : encodeValue(mock.input as SfValue),
    identity: mock.identity ?? null,
    locale: mock.locale ?? null,
    path: mock.path ?? null,
    methods,
    natives,
  };
  const id = sf().ctx(JSON.stringify(spec));
  for (const [service, table] of Object.entries(mock.services ?? {})) {
    for (const [method, answer] of Object.entries(table)) mocks.set(`${id}:${service}.${method}`, answer);
  }
  for (const [module, table] of Object.entries(mock.native ?? {})) {
    for (const [method, answer] of Object.entries(table)) mocks.set(`${id}:native:${module}.${method}`, answer);
  }
  return {
    id,
    locale: sf().locale(id),
    get session() {
      return decodeValue(JSON.parse(sf().session(id))) as Record<string, unknown>;
    },
    get trace() {
      return { calls: decodeValue(JSON.parse(sf().calls(id))) as unknown as ServiceCall[] };
    },
  };
}

/** Called by the runner when a body under test reaches a mocked service method. Answers synchronously: a mock is a function of its arguments or a mock function answering through `mockResolvedValue` or `mockRejectedValue`. */
function callMock(key: string, args: string): string {
  const answer = mocks.get(key);
  if (!answer) {
    console.error(`no mock for ${key}`);
    throw new Error(`no mock for ${key}`);
  }
  const result = answer(decodeValue(JSON.parse(args)) as never);
  if (result !== null && typeof result === "object" && typeof (result as { then?: unknown }).then === "function") {
    const settled = (result as { [SETTLED]?: { ok: boolean; value: unknown } })[SETTLED];
    if (!settled) throw new Error(`the mock for ${key} returned a promise; a mock answers synchronously or through a mock function's mockResolvedValue`);
    if (!settled.ok) throw settled.value;
    return JSON.stringify(encodeValue(settled.value as SfValue));
  }
  return JSON.stringify(encodeValue(result as SfValue));
}

// ---- the runner ----

type Mode = "run" | "skip" | "only" | "todo";

/** A test's or a hook's body: a function that may be async or may take a `done` callback. */
export type TestBody = (done: DoneCallback) => unknown;

export interface DoneCallback {
  (error?: unknown): void;
  fail(error?: unknown): void;
}

interface Block {
  name: string;
  parent: Block | null;
  mode: Mode;
  beforeAll: TestBody[];
  afterAll: TestBody[];
  beforeEach: TestBody[];
  afterEach: TestBody[];
  entered: boolean;
  failure: { error: unknown } | null;
}

interface Case {
  name: string;
  block: Block;
  body: TestBody | null;
  mode: Mode;
}

function block(name: string, parent: Block | null, mode: Mode): Block {
  return { name, parent, mode, beforeAll: [], afterAll: [], beforeEach: [], afterEach: [], entered: false, failure: null };
}

const root = block("", null, "run");
let current = root;
const cases: Case[] = [];
const entered: Block[] = [];

function chain(from: Block): Block[] {
  const out: Block[] = [];
  for (let at: Block | null = from; at; at = at.parent) out.unshift(at);
  return out;
}

function fullName(c: Case): string {
  return [...chain(c.block).slice(1).map((b) => b.name), c.name].join(" > ");
}

function anyOnly(): boolean {
  return cases.some((c) => c.mode === "only" || chain(c.block).some((b) => b.mode === "only"));
}

function modeOf(c: Case, only: boolean): "run" | "skip" | "todo" {
  if (c.mode === "todo") return "todo";
  const blocks = chain(c.block);
  if (c.mode === "skip" || blocks.some((b) => b.mode === "skip")) return "skip";
  if (only && c.mode !== "only" && !blocks.some((b) => b.mode === "only")) return "skip";
  return "run";
}

function invoke(body: TestBody): Promise<unknown> {
  if (body.length > 0) {
    return new Promise((resolve, reject) => {
      const done = ((error?: unknown) => (error === undefined || error === null ? resolve(undefined) : reject(error))) as DoneCallback;
      done.fail = (error?: unknown) => reject(error ?? new Error("done.fail()"));
      try {
        (body as (done: DoneCallback) => unknown)(done);
      } catch (e) {
        reject(e);
      }
    });
  }
  return Promise.resolve().then(() => (body as () => unknown)());
}

async function runCase(c: Case): Promise<void> {
  const blocks = chain(c.block);
  for (const b of blocks) {
    if (b.entered) continue;
    b.entered = true;
    entered.push(b);
    try {
      for (const hook of b.beforeAll) await invoke(hook);
    } catch (error) {
      b.failure = { error };
    }
  }
  const failed = blocks.find((b) => b.failure);
  if (failed?.failure) throw failed.failure.error;
  resetAssertions();
  let failure: { error: unknown } | null = null;
  try {
    for (const b of blocks) for (const hook of b.beforeEach) await invoke(hook);
    if (c.body) await invoke(c.body);
    verifyAssertions();
  } catch (error) {
    failure = { error };
  }
  for (const b of [...blocks].reverse()) {
    for (const hook of b.afterEach) {
      try {
        await invoke(hook);
      } catch (error) {
        failure ??= { error };
      }
    }
  }
  if (failure) throw failure.error;
}

/** Runs the `afterAll` hooks of every block a test entered, innermost first, after the file's last test. */
async function finish(): Promise<void> {
  const failures: string[] = [];
  for (const b of [...entered].reverse()) {
    for (const hook of b.afterAll) {
      try {
        await invoke(hook);
      } catch (error) {
        failures.push(`${b.name || "the file"}: ${error instanceof Error ? error.message : show(error)}`);
      }
    }
  }
  if (failures.length > 0) throw new AssertionError(`afterAll failed\n${failures.join("\n")}`);
}

Object.assign(globalThis, {
  __sf_call: callMock,
  __sf_tests: () => cases.map(fullName),
  __sf_modes: () => {
    const only = anyOnly();
    return cases.map((c) => modeOf(c, only));
  },
  __sf_run: (i: number) => {
    const run = runCase(cases[i]);
    run.catch(() => {});
    return run;
  },
  __sf_finish: () => {
    const run = finish();
    run.catch(() => {});
    return run;
  },
});

/** The rows of an `each` table as the arguments each case is called with: an array row is spread, any other row is the one argument and a template table's rows are objects keyed by its heading. */
function rowsOf(table: unknown, values: unknown[]): unknown[][] {
  if (Array.isArray(table) && Object.prototype.hasOwnProperty.call(table, "raw")) {
    const headings = String(table[0]).split("|").map((h) => h.trim()).filter(Boolean);
    const rows: unknown[][] = [];
    for (let i = 0; i < values.length; i += headings.length) {
      const row: Record<string, unknown> = {};
      headings.forEach((heading, j) => (row[heading] = values[i + j]));
      rows.push([row]);
    }
    return rows;
  }
  if (!Array.isArray(table)) throw new Error("each takes an array of rows or a template table");
  return table.map((row) => (Array.isArray(row) ? row : [row]));
}

/** An `each` case's name: printf placeholders take the arguments in order, `%#` is the row's index, `%$` its number and `$name` a field of an object row. */
function titled(name: string, args: unknown[], index: number): string {
  let next = 0;
  let out = name.replace(/%([sdifjoOp#$%])/g, (whole, flag: string) => {
    if (flag === "%") return "%";
    if (flag === "#") return String(index);
    if (flag === "$") return String(index + 1);
    if (next >= args.length) return whole;
    const value = args[next++];
    switch (flag) {
      case "s":
        return typeof value === "string" ? value : show(value);
      case "d":
      case "i":
        return typeof value === "bigint" ? String(value) : String(Math.trunc(Number(value)));
      case "f":
        return String(Number(value));
      case "j":
        return JSON.stringify(value);
      default:
        return show(value);
    }
  });
  const row = args.length === 1 && typeof args[0] === "object" && args[0] !== null ? args[0] : null;
  if (row) {
    out = out.replace(/\$([A-Za-z_][\w.]*)/g, (whole, path: string) => {
      let at: unknown = row;
      for (const key of path.split(".")) {
        if (at === null || typeof at !== "object" || !(key in (at as object))) return whole;
        at = (at as Record<string, unknown>)[key];
      }
      return typeof at === "string" ? at : show(at);
    });
  }
  return out;
}

type Register = (name: string, body?: TestBody, timeout?: number) => void;

export interface Each {
  (table: readonly unknown[]): (name: string, body: (...args: any[]) => unknown, timeout?: number) => void;
  (strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => unknown, timeout?: number) => void;
}

function eachOf(register: (name: string, body: TestBody) => void): Each {
  return ((table: unknown, ...values: unknown[]) =>
    (name: string, body: (...args: unknown[]) => unknown) => {
      rowsOf(table, values).forEach((args, index) => register(titled(name, args, index), () => body(...args)));
    }) as Each;
}

export interface TestApi extends Register {
  only: Register & { each: Each };
  skip: Register & { each: Each };
  todo(name: string): void;
  each: Each;
  skipIf(condition: unknown): Register;
  runIf(condition: unknown): Register;
  /** Passes when the body fails. */
  fails: Register;
  /** Runs in order like any other test: the runner runs one test at a time. */
  concurrent: Register;
}

function registerAs(mode: Mode): Register {
  return (name, body) => {
    cases.push({ name, block: current, body: body ?? null, mode: body ? mode : "todo" });
  };
}

function inverted(body: TestBody): TestBody {
  return async () => {
    let threw = false;
    try {
      await invoke(body);
    } catch {
      threw = true;
    }
    if (!threw) throw new AssertionError("test.fails: the test passed");
  };
}

export const test: TestApi = Object.assign(registerAs("run"), {
  only: Object.assign(registerAs("only"), { each: eachOf(registerAs("only")) }),
  skip: Object.assign(registerAs("skip"), { each: eachOf(registerAs("skip")) }),
  todo: (name: string) => registerAs("todo")(name),
  each: eachOf(registerAs("run")),
  skipIf: (condition: unknown) => (condition ? registerAs("skip") : registerAs("run")),
  runIf: (condition: unknown) => (condition ? registerAs("run") : registerAs("skip")),
  fails: ((name: string, body?: TestBody) => registerAs("run")(name, body ? inverted(body) : undefined)) as Register,
  concurrent: registerAs("run"),
});

export const it: TestApi = test;
export const xit: Register = test.skip;
export const xtest: Register = test.skip;
export const fit: Register = test.only;

type Group = (name: string, body: () => void) => void;

export interface DescribeApi extends Group {
  only: Group & { each: DescribeEach };
  skip: Group & { each: DescribeEach };
  each: DescribeEach;
  skipIf(condition: unknown): Group;
  runIf(condition: unknown): Group;
  concurrent: Group;
}

export interface DescribeEach {
  (table: readonly unknown[]): (name: string, body: (...args: any[]) => void) => void;
  (strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => void) => void;
}

function groupAs(mode: Mode): Group {
  return (name, body) => {
    const outer = current;
    current = block(name, outer, mode);
    try {
      const returned = body() as unknown;
      if (returned && typeof (returned as { then?: unknown }).then === "function") throw new Error(`describe(${JSON.stringify(name)}): a describe body runs at once and returns nothing; put what it awaits in a test or a hook`);
    } finally {
      current = outer;
    }
  };
}

function describeEachOf(group: Group): DescribeEach {
  return ((table: unknown, ...values: unknown[]) =>
    (name: string, body: (...args: unknown[]) => void) => {
      rowsOf(table, values).forEach((args, index) => group(titled(name, args, index), () => body(...args)));
    }) as DescribeEach;
}

export const describe: DescribeApi = Object.assign(groupAs("run"), {
  only: Object.assign(groupAs("only"), { each: describeEachOf(groupAs("only")) }),
  skip: Object.assign(groupAs("skip"), { each: describeEachOf(groupAs("skip")) }),
  each: describeEachOf(groupAs("run")),
  skipIf: (condition: unknown) => (condition ? groupAs("skip") : groupAs("run")),
  runIf: (condition: unknown) => (condition ? groupAs("run") : groupAs("skip")),
  concurrent: groupAs("run"),
});

export const xdescribe: Group = describe.skip;
export const fdescribe: Group = describe.only;

/** Runs `body` once, before the first test of the enclosing `describe` or file that runs. The timeout is accepted and ignored: time does not pass on its own. */
export function beforeAll(body: TestBody, _timeout?: number): void {
  current.beforeAll.push(body);
}

/** Runs `body` once, after the file's last test, for every `describe` a test ran in. */
export function afterAll(body: TestBody, _timeout?: number): void {
  current.afterAll.push(body);
}

/** Runs `body` before each test of the enclosing `describe` or file, outer hooks first. */
export function beforeEach(body: TestBody, _timeout?: number): void {
  current.beforeEach.push(body);
}

/** Runs `body` after each test of the enclosing `describe` or file, inner hooks first, whether the test passed or not. */
export function afterEach(body: TestBody, _timeout?: number): void {
  current.afterEach.push(body);
}

export interface Vi {
  fn: typeof fn;
  spyOn: typeof spyOn;
  isMockFunction: typeof isMockFunction;
  mocked<T>(value: T): T;
  clearAllMocks: typeof clearAllMocks;
  resetAllMocks: typeof resetAllMocks;
  restoreAllMocks: typeof restoreAllMocks;
  useFakeTimers(): Vi;
  useRealTimers(): Vi;
  isFakeTimers(): boolean;
  advanceTimersByTime(ms: number): Promise<void>;
  advanceTimersByTimeAsync(ms: number): Promise<void>;
  waitFor<T>(callback: () => T | Promise<T>, options?: WaitForOptions): Promise<T>;
}

/** Mock functions, spies and the clock. Timers are always fake, since the clock moves only when a test moves it, so `advanceTimersByTime` returns a promise to await. */
export const vi: Vi = {
  fn,
  spyOn,
  isMockFunction,
  mocked: <T>(value: T): T => value,
  clearAllMocks,
  resetAllMocks,
  restoreAllMocks,
  useFakeTimers() {
    return vi;
  },
  useRealTimers() {
    return vi;
  },
  isFakeTimers: () => true,
  advanceTimersByTime: (ms: number) => advance(ms),
  advanceTimersByTimeAsync: (ms: number) => advance(ms),
  waitFor: <T>(callback: () => T | Promise<T>, options?: WaitForOptions) => waitFor(callback, options),
};

/** The same object as `vi`, under its other name. */
export const jest: Vi = vi;

export function equal(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  const [big, num] = typeof a === "bigint" ? [a, b] : [b, a];
  if (typeof big === "bigint" && typeof num === "number" && Number.isInteger(num) && BigInt(num) === big) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => equal(x, b[i]));
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && equal((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

/** The assertions specs used before `expect`, kept so code outside this repository keeps running. */
export const assert = {
  ok(value: unknown, message?: string): void {
    if (!value) throw new AssertionError(message ?? `assert.ok: ${show(value)}`);
  },
  equal(actual: unknown, expected: unknown, message?: string): void {
    if (!equal(actual, expected)) throw new AssertionError(`${message ?? "assert.equal"}\n  actual:   ${show(actual)}\n  expected: ${show(expected)}`);
  },
  /** `actual` holds `pattern`: contains it when a string, matches it when a RegExp. */
  match(actual: string, pattern: string | RegExp, message?: string): void {
    const hit = typeof actual === "string" && (typeof pattern === "string" ? actual.includes(pattern) : pattern.test(actual));
    if (!hit) throw new AssertionError(`${message ?? "assert.match"}\n  actual:  ${show(actual)}\n  pattern: ${show(pattern instanceof RegExp ? String(pattern) : pattern)}`);
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

// ---- rendering and loading ----

export interface Rendered extends BoundQueries {
  container: HTMLElement;
  baseElement: HTMLElement;
  root: Root;
  /** The module id the server rendered and React hydrated over; `null` when the component mounted fresh. */
  hydrated: string | null;
  unmount(): void;
  /** Renders `element` into the same root and settles. */
  rerender(element: ReactElement): Promise<void>;
  asFragment(): DocumentFragment;
  debug(element?: Element, maxLength?: number): void;
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
  setLocale(options.ctx?.locale ?? sf().locale(0));
  const container = document.createElement("div");
  document.body.appendChild(container);
  const module = options.hydrate === false ? null : await moduleOf(element.type);
  const rendered = module === null ? null : sf().render(module, JSON.stringify(encodeValue(element.props as SfValue)));
  // React is reached only here, so a spec suite for an application with no
  // React in its import map never asks for it.
  const [{ createRoot, hydrateRoot }, { withHoisted }] = await Promise.all([import("react-dom/client"), import("./react.js")]);
  let root: Root;
  let hydrated: string | null = null;
  if (rendered !== null) {
    const { html, hoisted } = JSON.parse(rendered) as { html: string; hoisted: SfValue };
    container.innerHTML = html;
    root = hydrateRoot(container, withHoisted(decodeValue(hoisted) as Hoisted, element));
    hydrated = module;
  } else {
    root = createRoot(container);
    root.render(element);
  }
  await settle();
  return {
    ...within(container),
    container,
    baseElement: document.body,
    root,
    hydrated,
    unmount() {
      root.unmount();
      container.remove();
    },
    async rerender(next: ReactElement) {
      root.render(next);
      await settle();
    },
    asFragment() {
      const template = document.createElement("template");
      template.innerHTML = container.innerHTML;
      return template.content;
    },
    debug(target?: Element, maxLength?: number) {
      console.log(prettyDOM(target ?? container, maxLength));
    },
  };
}

/** Renders a component that calls `hook` and holds what it returned in `result.current`, so a hook is tested without a page around it. */
export async function renderHook<Result, Props = undefined>(
  hook: (props: Props) => Result,
  options: { initialProps?: Props; ctx?: TestCtx; wrapper?: ComponentType<{ children: ReactNode }> } = {},
): Promise<{ result: { current: Result }; rerender(props?: Props): Promise<void>; unmount(): void }> {
  const { createElement } = await import("react");
  const result = { current: undefined as Result };
  function Probe({ props }: { props: Props }): null {
    result.current = hook(props);
    return null;
  }
  const element = (props: Props): ReactElement => {
    const probe = createElement(Probe, { props });
    return options.wrapper ? createElement(options.wrapper, null, probe) : probe;
  };
  const rendered = await render(element(options.initialProps as Props), { ctx: options.ctx, hydrate: false });
  return {
    result,
    rerender: (props?: Props) => rendered.rerender(element((props ?? options.initialProps) as Props)),
    unmount: () => rendered.unmount(),
  };
}

/** Runs `body` and settles, React's `act` for code that changes state outside an event the harness dispatched. */
export async function act<T>(body: () => T | Promise<T>): Promise<T> {
  const out = await body();
  await settle();
  return out;
}

/** Ends every island the body holds and empties it, which the runner also does after every test. */
export function cleanup(): void {
  discard(document.body);
  document.body.innerHTML = "";
}

/** Loads a route the way a browser does: the document the host renders for `path` under `ctx`, its islands mounted, navigation enabled, so a click on a link is a client navigation. The islands of the page showing until now are ended first, as leaving a page ends them in a browser. Needs the configuration beside the app, since the host that renders is the one that serves. */
export async function load(path: string, options: { ctx?: TestCtx } = {}): Promise<{ status: number; path: string }> {
  sf().use(options.ctx?.id ?? 0);
  let res = await fetch(path);
  for (let hops = 0; hops < 5 && res.status >= 300 && res.status < 400; hops++) {
    const to = res.headers.get("location");
    if (!to) break;
    path = to;
    res = await fetch(path);
  }
  const html = await res.text();
  if (!/<!doctype/i.test(html.slice(0, 256))) throw new AssertionError(`load ${show(path)}: HTTP ${res.status}: ${html.trim()}`);
  discard(document);
  sf().load(html, path);
  clearRouterCache();
  reset();
  const late = applyFills();
  boot();
  enableNavigation();
  for (const run of late) run();
  await settle();
  return { status: res.status, path };
}

/** What a browser's script engine does with a streamed document: moves each resolved template into its slot and returns what its fill script would have said about the head and the store, to run once the document's own seed is in. linkedom runs no scripts, so the runner does this by hand. */
function applyFills(): (() => void)[] {
  const late: (() => void)[] = [];
  for (const template of Array.from(document.querySelectorAll("template[data-sf-fill]"))) {
    const id = template.getAttribute("data-sf-fill");
    const slot = document.querySelector(`[data-sf-slot="${id}"]`);
    const script = template.nextElementSibling;
    if (slot) slot.replaceWith((template as HTMLTemplateElement).content);
    template.remove();
    if (script?.tagName !== "SCRIPT" || !script.textContent?.startsWith("__sfFill(")) continue;
    for (const call of script.textContent.split(";__sf").slice(1)) {
      const open = call.indexOf("(");
      const body = call.slice(open + 1, call.lastIndexOf(")"));
      if (call.startsWith("Head(")) late.push(() => applyHead(JSON.parse(body)));
      if (call.startsWith("Store(")) late.push(() => seed(decodeValue(JSON.parse(body)) as { [key: string]: SfValue }));
    }
    script.remove();
  }
  return late;
}
