import type { ReactElement } from "react";
import { suspendTerminal } from "@snapfire/fsr/terminal/core";
/** The terminal's size in cells. */
export interface WindowSize {
	columns: number;
	rows: number;
}
/** The terminal's size, which the terminal keeps in the store. */
export declare function useWindowSize(): WindowSize;
/** The terminal's output: its size, which `stdout.on("resize", f)` hears change, plus `write`, which prints a line above the page. */
export declare function useStdout(): {
	stdout: WindowSize & {
		$process: string;
	};
	write: (text: string) => void;
};
/** The terminal's error output, printed above the page as `useStdout`'s is. */
export declare function useStderr(): {
	stderr: WindowSize;
	write: (text: string) => void;
};
/** A terminal page has no screen reader to tell it about. */
export declare function useIsScreenReaderEnabled(): boolean;
/** The focus across the page: the focused id, which the terminal keeps in the store, and the acts that move it. */
export interface FocusManager {
	activeId: string | null;
	focus: (id: string) => void;
	focusNext: () => void;
	focusPrevious: () => void;
	enableFocus: () => void;
	disableFocus: () => void;
}
export declare function useFocusManager(): FocusManager;
/** Ink's cursor control. */
export declare function useCursor(): {
	setCursorPosition: (position: {
		x: number;
		y: number;
	} | undefined) => void;
};
/** Makes the component focusable: its id, the one given or its own, is handed to the terminal, which moves the focus through them in render order; `isFocused` compares it with the focused id the terminal keeps in the store. */
export declare function useFocus(options?: {
	id?: string;
	autoFocus?: boolean;
	isActive?: boolean;
}): {
	isFocused: boolean;
	focus: (id: string) => void;
};
/** Where a Box lies and how large it is, in cells. */
export interface BoxMetrics {
	width: number;
	height: number;
	left: number;
	top: number;
	clientWidth: number;
	clientHeight: number;
	hasMeasured: boolean;
}
/** The layout of the Box `ref` is on, as the terminal measured it after its last layout, by the id a terminal `useRef` holds; zeros until it is measured. */
export declare function useBoxMetrics(ref: {
	readonly current: unknown;
}): BoxMetrics;
/** Ink's `Static`: its items in a column above the rest of the page, each drawn by the function it holds; `style` holds Box props. */
export declare function Static<T>({ items, style, children }: {
	items: T[];
	style?: Record<string, unknown>;
	children: (item: T, index: number) => unknown;
}): ReactElement;
/** The page's control of the terminal: `exit` quits, and `suspendTerminal(f)` gives the terminal up while the promise `f` returns is pending. */
export declare function useApp(): {
	exit: (error?: unknown) => void;
	suspendTerminal: typeof suspendTerminal;
};
