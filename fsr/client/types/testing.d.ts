import type { ReactElement } from "react";
import { type Root } from "react-dom/client";
type Method = (args: never) => unknown;
export interface Mock<Input = unknown> {
	session?: Record<string, unknown>;
	services?: Record<string, Record<string, Method>>;
	input?: Input;
	params?: Record<string, string>;
	query?: Record<string, string>;
	identity?: {
		subject: string;
		claims?: Record<string, unknown>;
	};
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
	readonly trace: {
		calls: ServiceCall[];
	};
}
export declare function ctx(mock?: Mock): TestCtx;
export declare function test(name: string, body: () => Promise<void> | void): void;
export declare class AssertionError extends Error {
	constructor(message: string);
}
/** Values the way a test reads them: `1n` and `1` stay distinct, strings are quoted. */
export declare function show(value: unknown, depth?: number): string;
export declare function equal(a: unknown, b: unknown): boolean;
export declare const assert: {
	ok(value: unknown, message?: string): void;
	equal(actual: unknown, expected: unknown, message?: string): void;
	throws(run: () => unknown, match?: string | RegExp): void;
	rejects(run: Promise<unknown> | (() => Promise<unknown>), match?: string | RegExp): Promise<void>;
};
export interface Rendered {
	container: HTMLElement;
	root: Root;
	/** The module id the server rendered and React hydrated over; `null` when the component mounted fresh. */
	hydrated: string | null;
	unmount(): void;
}
/** Runs everything that happens now: microtasks, action calls, their re-renders and timers already due. A timer set for later waits for `advance`. */
export declare function settle(): Promise<void>;
/** Moves the clock `ms` forward and settles, so timers due by then fire in order. Time never passes on its own. */
export declare function advance(ms: number): Promise<void>;
/** Mounts `element` under a fresh container. A page the server renders is hydrated over its own markup, so a mismatch fails here the way it would in a browser; anything else mounts fresh. */
export declare function render(element: ReactElement, options?: {
	ctx?: TestCtx;
	hydrate?: boolean;
}): Promise<Rendered>;
/** Loads a route the way a browser does: the document the host renders for `path` under `ctx`, its islands mounted, navigation enabled, so a click on a link is a client navigation. Needs the configuration beside the app, since the host that renders is the one that serves. */
export declare function load(path: string, options?: {
	ctx?: TestCtx;
}): Promise<{
	status: number;
	path: string;
}>;
type Matcher = string | RegExp;
/** Queries over the document, by the text an element itself holds, its label, its placeholder or its `data-testid`. */
export declare const screen: {
	getByText(matcher: Matcher, root?: ParentNode): HTMLElement;
	queryByText(matcher: Matcher, root?: ParentNode): HTMLElement | null;
	getAllByText(matcher: Matcher, root?: ParentNode): HTMLElement[];
	getByLabelText(matcher: Matcher, root?: ParentNode): HTMLElement;
	getByPlaceholderText(matcher: Matcher, root?: ParentNode): HTMLElement;
	getByTestId(id: string, root?: ParentNode): HTMLElement;
};
/** Dispatches DOM events and settles the engine after each, so the assertion that follows sees the re-render. */
export declare const fireEvent: {
	click(el: Element): Promise<void>;
	change(el: Element, value: string): Promise<void>;
	submit(el: Element): Promise<void>;
	keyDown(el: Element, key: string): Promise<void>;
};
export {};
