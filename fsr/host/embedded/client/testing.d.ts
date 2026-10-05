import type { ComponentType, ReactElement, ReactNode } from "react";
import type { Root } from "react-dom/client";
import { type Props } from "./boot.js";
import { clearAllMocks, fn, isMockFunction, resetAllMocks, restoreAllMocks, spyOn } from "./expect.js";
import { type BoundQueries, type WaitForOptions } from "./queries.js";
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
	identity?: {
		subject: string;
		claims?: Record<string, unknown>;
	};
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
	readonly trace: {
		calls: ServiceCall[];
	};
}
export declare function ctx(mock?: Mock): TestCtx;
/** A test's or a hook's body: a function that may be async or may take a `done` callback. */
export type TestBody = (done: DoneCallback) => unknown;
export interface DoneCallback {
	(error?: unknown): void;
	fail(error?: unknown): void;
}
type Register = (name: string, body?: TestBody, timeout?: number) => void;
export interface Each {
	(table: readonly unknown[]): (name: string, body: (...args: any[]) => unknown, timeout?: number) => void;
	(strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => unknown, timeout?: number) => void;
}
export interface TestApi extends Register {
	only: Register & {
		each: Each;
	};
	skip: Register & {
		each: Each;
	};
	todo(name: string): void;
	each: Each;
	skipIf(condition: unknown): Register;
	runIf(condition: unknown): Register;
	/** Passes when the body fails. */
	fails: Register;
	/** Runs in order like any other test: the runner runs one test at a time. */
	concurrent: Register;
}
export declare const test: TestApi;
export declare const it: TestApi;
export declare const xit: Register;
export declare const xtest: Register;
export declare const fit: Register;
type Group = (name: string, body: () => void) => void;
export interface DescribeApi extends Group {
	only: Group & {
		each: DescribeEach;
	};
	skip: Group & {
		each: DescribeEach;
	};
	each: DescribeEach;
	skipIf(condition: unknown): Group;
	runIf(condition: unknown): Group;
	concurrent: Group;
}
export interface DescribeEach {
	(table: readonly unknown[]): (name: string, body: (...args: any[]) => void) => void;
	(strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => void) => void;
}
export declare const describe: DescribeApi;
export declare const xdescribe: Group;
export declare const fdescribe: Group;
/** Runs `body` once, before the first test of the enclosing `describe` or file that runs. The timeout is accepted and ignored: time does not pass on its own. */
export declare function beforeAll(body: TestBody, _timeout?: number): void;
/** Runs `body` once, after the file's last test, for every `describe` a test ran in. */
export declare function afterAll(body: TestBody, _timeout?: number): void;
/** Runs `body` before each test of the enclosing `describe` or file, outer hooks first. */
export declare function beforeEach(body: TestBody, _timeout?: number): void;
/** Runs `body` after each test of the enclosing `describe` or file, inner hooks first, whether the test passed or not. */
export declare function afterEach(body: TestBody, _timeout?: number): void;
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
export declare const vi: Vi;
/** The same object as `vi`, under its other name. */
export declare const jest: Vi;
export declare function equal(a: unknown, b: unknown): boolean;
/** The assertions specs used before `expect`, kept so code outside this repository keeps running. */
export declare const assert: {
	ok(value: unknown, message?: string): void;
	equal(actual: unknown, expected: unknown, message?: string): void;
	/** `actual` holds `pattern`: contains it when a string, matches it when a RegExp. */
	match(actual: string, pattern: string | RegExp, message?: string): void;
	throws(run: () => unknown, match?: string | RegExp): void;
	rejects(run: Promise<unknown> | (() => Promise<unknown>), match?: string | RegExp): Promise<void>;
};
export interface Rendered extends BoundQueries {
	container: HTMLElement;
	baseElement: HTMLElement;
	root: Root;
	/** The module id the server rendered and the island's adapter hydrated over; `null` when the component mounted fresh or is composition. */
	hydrated: string | null;
	/** The module id of a component composition renders: the server's markup is the whole of it and only the islands inside mount. */
	composed: string | null;
	unmount(): void;
	/** Renders `element` into the same root and settles. */
	rerender(element: Placed): Promise<void>;
	asFragment(): DocumentFragment;
	debug(element?: Element, maxLength?: number): void;
}
/** Names a page or layout composition renders, so `render` writes the server's markup for it and mounts only the islands inside. The spec runner's boot calls it for each one. */
export declare function registerComposition(moduleId: string, loader: () => Promise<unknown>): void;
/** What a spec's JSX builds, whichever runtime built it: React's element, FSR's or any value with a component `type` and its `props`. */
export type Placed = ReactElement | {
	type: unknown;
	props: Record<string, unknown>;
};
/** Mounts `element` under a fresh container. A page or layout is composition: the server's markup is written and only the islands inside mount. A registered island of any framework mounts through its own adapter, hydrating over the server's markup when its module lowers, so a mismatch fails here the way it would in a browser. Any other React element mounts fresh in a React root. */
export declare function render(element: Placed, options?: {
	ctx?: TestCtx;
	hydrate?: boolean;
}): Promise<Rendered>;
/** Renders a component that calls `hook` and holds what it returned in `result.current`, so a hook is tested without a page around it. */
export declare function renderHook<
	Result,
	Props = undefined
>(hook: (props: Props) => Result, options?: {
	initialProps?: Props;
	ctx?: TestCtx;
	wrapper?: ComponentType<{
		children: ReactNode;
	}>;
}): Promise<{
	result: {
		current: Result;
	};
	rerender(props?: Props): Promise<void>;
	unmount(): void;
}>;
/** Runs `body` and settles, React's `act` for code that changes state outside an event the harness dispatched. */
export declare function act<T>(body: () => T | Promise<T>): Promise<T>;
/** Ends every island the body holds and empties it, which the runner also does after every test. */
export declare function cleanup(): void;
/** Loads a route the way a browser does: the document the host renders for `path` under `ctx`, its islands mounted, the application's entry module evaluated again and navigation enabled, so a click on a link is a client navigation and whatever the entry wires is wired to this document. The islands of the page showing until now are ended first, as leaving a page ends them in a browser. Needs the configuration beside the app, since the host that renders is the one that serves. */
export declare function load(path: string, options?: {
	ctx?: TestCtx;
}): Promise<{
	status: number;
	path: string;
}>;
