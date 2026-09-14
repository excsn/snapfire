/** What a query compares an element's text with: the text itself, a pattern or a function of the text and the element. */
export type Matcher = string | RegExp | ((content: string, element: Element | null) => boolean);
export interface MatcherOptions {
	/** `false` matches a string case-insensitively anywhere in the text. Defaults to `true`, the whole text. */
	exact?: boolean;
	normalizer?: (text: string) => string;
	trim?: boolean;
	collapseWhitespace?: boolean;
}
export interface SelectorMatcherOptions extends MatcherOptions {
	/** Only elements matching this selector. */
	selector?: string;
	/** Elements matching this selector are never matched; `false` ignores nothing. Defaults to `"script, style"`. */
	ignore?: string | false;
}
export interface ByRoleOptions {
	name?: Matcher;
	description?: Matcher;
	/** Includes elements hidden from the accessibility tree. */
	hidden?: boolean;
	level?: number;
	checked?: boolean;
	selected?: boolean;
	pressed?: boolean;
	expanded?: boolean;
	current?: boolean | string;
	busy?: boolean;
	/** Matches any role an element's `role` attribute lists rather than the first. */
	queryFallbacks?: boolean;
}
export interface WaitForOptions {
	/** How long, on the harness clock, before the wait gives up. Defaults to `configure`'s `asyncUtilTimeout`, 1000. */
	timeout?: number;
	/** How far the clock moves between tries. Defaults to 50. */
	interval?: number;
	onTimeout?: (error: Error) => Error;
}
declare const config: {
	testIdAttribute: string;
	asyncUtilTimeout: number;
};
/** Sets the attribute `ByTestId` reads and the default wait timeout. */
export declare function configure(next: Partial<typeof config>): void;
export declare class TestingLibraryElementError extends Error {
	constructor(message: string);
}
export declare function getDefaultNormalizer(options?: {
	trim?: boolean;
	collapseWhitespace?: boolean;
}): (text: string) => string;
/** The text an element holds itself: its text node children, joined. */
export declare function nodeText(el: Element): string;
/** Whether `el` is left out of the accessibility tree: hidden by an attribute, by `aria-hidden` or by an inline style, itself or through an ancestor. The runner lays nothing out, so a stylesheet's `display: none` is not seen. */
export declare function isInaccessible(el: Element): boolean;
/** The roles an element answers to: the first token of its `role` attribute, every token with `fallbacks`, else the implicit role. */
export declare function rolesOf(el: Element, fallbacks?: boolean): string[];
/** The element's accessible name, the way a screen reader would announce it: `aria-labelledby`, `aria-label`, its labels, its alt text, its content where its role takes a name from content, then its title. */
export declare function accessibleName(el: Element): string;
/** The text `aria-describedby` points at, else the title when the title did not become the name. */
export declare function accessibleDescription(el: Element): string;
/** Resolves once `callback` stops throwing, retrying after everything due has run and then after moving the clock by `interval`, until `timeout` has passed on the harness clock. The clock moves only because this moves it, so timers in the page fire as the wait goes on. */
export declare function waitFor<T>(callback: () => T | Promise<T>, options?: WaitForOptions): Promise<T>;
/** Resolves once the element, every element in the list or whatever `callback` returns is gone from the document. Refuses one that is not there to begin with. */
export declare function waitForElementToBeRemoved(target: Element | Element[] | null | (() => Element | Element[] | null), options?: WaitForOptions): Promise<void>;
/** The eight queries of each kind: `getBy`, `getAllBy`, `queryBy`, `queryAllBy`, `findBy`, `findAllBy`. */
export interface BoundQueries {
	getByRole(role: Matcher, options?: ByRoleOptions): HTMLElement;
	getAllByRole(role: Matcher, options?: ByRoleOptions): HTMLElement[];
	queryByRole(role: Matcher, options?: ByRoleOptions): HTMLElement | null;
	queryAllByRole(role: Matcher, options?: ByRoleOptions): HTMLElement[];
	findByRole(role: Matcher, options?: ByRoleOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByRole(role: Matcher, options?: ByRoleOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement;
	getAllByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
	queryByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement | null;
	queryAllByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
	findByText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement;
	getAllByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
	queryByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement | null;
	queryAllByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
	findByLabelText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByLabelText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
	getAllByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	queryByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
	queryAllByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	findByPlaceholderText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByPlaceholderText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
	getAllByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	queryByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
	queryAllByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	findByAltText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByAltText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
	getAllByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	queryByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
	queryAllByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	findByTitle(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByTitle(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
	getAllByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	queryByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
	queryAllByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	findByDisplayValue(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByDisplayValue(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
	getByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
	getAllByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	queryByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
	queryAllByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
	findByTestId(id: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
	findAllByTestId(id: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
}
/** Markup for a failure message or a log: `node`'s outer HTML, the document's body by default, cut at `maxLength`. */
export declare function prettyDOM(node?: Element | Document | null, maxLength?: number): string;
/** Logs every accessible element under `container` with its role and name. */
export declare function logRoles(container?: ParentNode): void;
export interface Screen extends BoundQueries {
	debug(element?: Element | Element[] | null, maxLength?: number): void;
	logTestingPlaygroundURL(): void;
}
/** The queries over the document's body. */
export declare const screen: Screen;
/** The queries over `container` alone. */
export declare function within(container: ParentNode): BoundQueries;
/** The labelable control a label forwards a click to: the one its `for` names, else the first inside it. */
export declare function controlOf(label: Element): Element | null;
export {};
