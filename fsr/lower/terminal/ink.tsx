// Ink's API for a terminal page, written over FSR's terminal primitives. The
// build reads this file wherever a page imports a name it exports from
// `@snapfire/fsr/terminal/ink`; every other name comes from the primitives.

import { useId } from "react";
import type { ReactElement } from "react";
import { action } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";
import { Box, emit, suspendTerminal } from "@snapfire/fsr/terminal/core";

/** The terminal's size in cells. */
export interface WindowSize {
  columns: number;
  rows: number;
}

/** The terminal's size, which the terminal keeps in the store. */
export function useWindowSize(): WindowSize {
  const [columns] = useStore("$terminal.columns", 80);
  const [rows] = useStore("$terminal.rows", 24);
  return { columns, rows };
}

/** The terminal's output: its size, which `stdout.on("resize", f)` hears change, plus `write`, which prints a line above the page. */
export function useStdout(): { stdout: WindowSize & { $process: string }; write: (text: string) => void } {
  const { columns, rows } = useWindowSize();
  return { stdout: { columns, rows, $process: "$terminal" }, write: action("$stdout") };
}

/** The terminal's error output, printed above the page as `useStdout`'s is. */
export function useStderr(): { stderr: WindowSize; write: (text: string) => void } {
  const { columns, rows } = useWindowSize();
  return { stderr: { columns, rows }, write: action("$stderr") };
}

/** A terminal page has no screen reader to tell it about. */
export function useIsScreenReaderEnabled(): boolean {
  return false;
}

/** The focus across the page: the focused id, which the terminal keeps in the store, and the acts that move it. */
export interface FocusManager {
  activeId: string | null;
  focus: (id: string) => void;
  focusNext: () => void;
  focusPrevious: () => void;
  enableFocus: () => void;
  disableFocus: () => void;
}

export function useFocusManager(): FocusManager {
  const [activeId] = useStore("$terminal.focus", null);
  return {
    activeId,
    focus: action("$focus"),
    focusNext: action("$focusNext"),
    focusPrevious: action("$focusPrevious"),
    enableFocus: action("$focusEnable"),
    disableFocus: action("$focusDisable"),
  };
}

/** Puts the terminal's cursor at `{ x, y }` from the corner of the page; the last call a render makes holds, and a render that makes none hides it. */
function setCursorPosition(position: { x: number; y: number } | undefined): void {
  emit("cursor", position ?? null);
}

/** Ink's cursor control. */
export function useCursor(): { setCursorPosition: (position: { x: number; y: number } | undefined) => void } {
  return { setCursorPosition };
}

/** Makes the component focusable: its id, the one given or its own, is handed to the terminal, which moves the focus through them in render order; `isFocused` compares it with the focused id the terminal keeps in the store. */
export function useFocus(options: { id?: string; autoFocus?: boolean; isActive?: boolean } = {}): { isFocused: boolean; focus: (id: string) => void } {
  const own = useId();
  const id = options.id ?? own;
  emit("focusable", { id, autoFocus: options.autoFocus ?? false, isActive: options.isActive ?? true });
  const [focused] = useStore("$terminal.focus", null);
  return { isFocused: focused === id, focus: action("$focus") };
}

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

const unmeasured: BoxMetrics = { width: 0, height: 0, left: 0, top: 0, clientWidth: 0, clientHeight: 0, hasMeasured: false };

/** The layout of the Box `ref` is on, as the terminal measured it after its last layout, by the id a terminal `useRef` holds; zeros until it is measured. */
export function useBoxMetrics(ref: { readonly current: unknown }): BoxMetrics {
  const [measured] = useStore("$terminal.metrics", {});
  const found = measured[(ref as { $id?: string }).$id];
  return found ? { ...found, hasMeasured: true } : unmeasured;
}

/** Ink's `Static`: its items in a column above the rest of the page, each drawn by the function it holds; `style` holds Box props. */
export function Static<T>({ items, style, children }: { items: T[]; style?: Record<string, unknown>; children: (item: T, index: number) => unknown }): ReactElement {
  return (
    <Box flexDirection="column" {...style}>
      {items.map((item, index) => children(item, index))}
    </Box>
  );
}

/** The page's control of the terminal: `exit` quits, and `suspendTerminal(f)` gives the terminal up while the promise `f` returns is pending. */
export function useApp(): { exit: (error?: unknown) => void; suspendTerminal: typeof suspendTerminal } {
  return { exit: action("$exit"), suspendTerminal };
}
