/** A value that decides equality itself: what `expect.any(Number)` and the other `expect.*` helpers build. */
export interface AsymmetricMatcher {
	$$typeof: symbol;
	asymmetricMatch(other: unknown): boolean;
	toString(): string;
}
/** `toEqual`'s equality: structural, a property holding `undefined` counted as absent, a bigint equal to the whole number it stands for. `strict` is `toStrictEqual`'s: `undefined` properties count, prototypes must match and a bigint is not a number. */
export declare function equals(a: unknown, b: unknown, strict?: boolean, seen?: [unknown, unknown][]): boolean;
/** A promise a mock function answered with through `mockResolvedValue` or `mockRejectedValue`, marked with how it settles, so a service mock can answer synchronously all the same. */
export declare const SETTLED: unique symbol;
export interface MockResult {
	type: "return" | "throw" | "incomplete";
	value: unknown;
}
export interface MockState<
	A extends unknown[],
	R
> {
	calls: A[];
	results: MockResult[];
	instances: unknown[];
	contexts: unknown[];
	invocationCallOrder: number[];
	readonly lastCall: A | undefined;
}
/** A function that records its calls, what `fn()` returns. */
export interface MockInstance<
	A extends unknown[] = any[],
	R = any
> {
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
/** A mock function, calling `impl` when given. */
export declare function fn<
	A extends unknown[] = any[],
	R = any
>(impl?: (...args: A) => R): MockInstance<A, R>;
export declare function isMockFunction(value: unknown): value is MockInstance;
/** Replaces `object[key]` with a mock function that calls the original until told otherwise; `mockRestore` puts the original back. `access` spies on a getter or a setter instead. */
export declare function spyOn<
	T extends object,
	K extends keyof T
>(object: T, key: K, access?: "get" | "set"): MockInstance;
export declare function clearAllMocks(): void;
export declare function resetAllMocks(): void;
export declare function restoreAllMocks(): void;
export interface MatcherResult {
	pass: boolean;
	message: () => string;
}
export interface MatcherState {
	isNot: boolean;
	promise: "" | "resolves" | "rejects";
	equals(a: unknown, b: unknown): boolean;
	utils: {
		stringify(value: unknown): string;
		printReceived(value: unknown): string;
		printExpected(value: unknown): string;
	};
}
export type MatcherFunction = (this: MatcherState, received: any, ...expected: any[]) => MatcherResult | Promise<MatcherResult>;
/** Called by the runner before each test. */
export declare function resetAssertions(): void;
/** Called by the runner after each test body: what `expect.assertions` and `expect.hasAssertions` asked for. */
export declare function verifyAssertions(): void;
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
	toHaveTextContent(text: string | RegExp, options?: {
		normalizeWhitespace?: boolean;
	}): R;
	toHaveAttribute(name: string, value?: unknown): R;
	toHaveClass(...names: (string | {
		exact?: boolean;
	})[]): R;
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
	resolves: Matchers<Promise<void>> & {
		not: Matchers<Promise<void>>;
	};
	rejects: Matchers<Promise<void>> & {
		not: Matchers<Promise<void>>;
	};
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
export declare const expect: Expect;
