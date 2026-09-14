/** Sets a form control's value the way a user would, past React's own value tracking, so the `input` event that follows is seen as a change. */
export declare function setValue(el: HTMLElement, value: string): void;
type Target = Element | Document | Window;
/** What an event is made with: its init fields, plus `target`, properties set on the element before it is dispatched, as in `fireEvent.change(input, { target: { value } })`. */
export type EventInit = Record<string, unknown> & {
	target?: Record<string, unknown>;
};
/** Each event `fireEvent` names: the DOM type, its constructor, whether it bubbles and whether it can be cancelled. */
declare const TABLE: Record<string, [string, string, boolean, boolean]>;
/** An event of the kind `name` names, ready to dispatch on `node`. */
export declare function createEvent(name: string, node: Target, init?: EventInit): Event;
type Fire = (node: Target, init?: EventInit | string) => Promise<boolean>;
export type FireEvent = ((node: Target, event: Event) => Promise<boolean>) & { [name in keyof typeof TABLE]: Fire };
/** Dispatches DOM events and settles the engine after each, so the assertion that follows sees the re-render. `change` also takes the new value as a string. A click runs a browser's default action: a checkbox toggles, a label clicks its control and a submit button submits its form. */
export declare const fireEvent: FireEvent;
export interface UserOptions {
	/** Ignored: the harness clock moves only when a test moves it, so there is nothing to wait between keys. */
	delay?: number | null;
	/** Skips the hover a click begins with. */
	skipHover?: boolean;
	/** Ignored, since time never passes on its own. */
	advanceTimers?: (ms: number) => unknown;
}
export interface UserEvent {
	click(element: Element, options?: {
		skipHover?: boolean;
	}): Promise<void>;
	dblClick(element: Element): Promise<void>;
	tripleClick(element: Element): Promise<void>;
	hover(element: Element): Promise<void>;
	unhover(element: Element): Promise<void>;
	tab(options?: {
		shift?: boolean;
	}): Promise<void>;
	type(element: Element, text: string, options?: {
		skipClick?: boolean;
	}): Promise<void>;
	keyboard(text: string): Promise<void>;
	clear(element: Element): Promise<void>;
	selectOptions(element: Element, values: string | Element | (string | Element)[]): Promise<void>;
	deselectOptions(element: Element, values: string | Element | (string | Element)[]): Promise<void>;
	upload(element: Element, files: unknown | unknown[]): Promise<void>;
	paste(text: string): Promise<void>;
}
/** A user at the keyboard and the pointer. `userEvent.setup()` gives a session holding its own modifier keys and hover; the same methods are on `userEvent` itself. Every method settles before it resolves. */
export declare const userEvent: UserEvent & {
	setup(options?: UserOptions): UserEvent;
};
export {};
