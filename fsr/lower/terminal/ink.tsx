// Ink's API for a terminal page, written over FSR's terminal primitives. The
// build reads this file wherever a page imports a name it exports from `ink`
// or `@snapfire/fsr/terminal`; every other name comes from the primitives.

import { useId } from "react";
import { action } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";
import { Box, emit } from "@snapfire/fsr/terminal/core";

/** The terminal's size, which the terminal keeps in the store. */
export function useWindowSize() {
  const [columns] = useStore("$terminal.columns", 80);
  const [rows] = useStore("$terminal.rows", 24);
  return { columns, rows };
}

/** The terminal's output: its size, and `write`, which prints a line above the page. */
export function useStdout() {
  const { columns, rows } = useWindowSize();
  return { stdout: { columns, rows }, write: action("$stdout") };
}

/** The terminal's error output, printed above the page as `useStdout`'s is. */
export function useStderr() {
  const { columns, rows } = useWindowSize();
  return { stderr: { columns, rows }, write: action("$stderr") };
}

/** A terminal page has no screen reader to tell it about. */
export function useIsScreenReaderEnabled() {
  return false;
}

/** The focus across the page: the focused id, which the terminal keeps in the store, and the acts that move it. */
export function useFocusManager() {
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
function setCursorPosition(position: { x: number; y: number } | undefined) {
  emit("cursor", position ?? null);
}

/** Ink's cursor control. */
export function useCursor() {
  return { setCursorPosition };
}

/** Makes the component focusable: its id, the one given or its own, is handed to the terminal, which moves the focus through them in render order; `isFocused` compares it with the focused id the terminal keeps in the store. */
export function useFocus(options: { id?: string; autoFocus?: boolean; isActive?: boolean } = {}) {
  const own = useId();
  const id = options.id ?? own;
  emit("focusable", { id, autoFocus: options.autoFocus ?? false, isActive: options.isActive ?? true });
  const [focused] = useStore("$terminal.focus", null);
  return { isFocused: focused === id, focus: action("$focus") };
}

const unmeasured = { width: 0, height: 0, left: 0, top: 0, clientWidth: 0, clientHeight: 0, hasMeasured: false };

/** The layout of the Box `ref` is on, as the terminal measured it after its last layout, by the id a terminal `useRef` holds; zeros until it is measured. */
export function useBoxMetrics(ref: { $id?: string }) {
  const [measured] = useStore("$terminal.metrics", {});
  const found = measured[ref.$id];
  return found ? { ...found, hasMeasured: true } : unmeasured;
}

/** Ink's `Static`: its items in a column above the rest of the page, each drawn by the function it holds; `style` holds Box props. */
export function Static<T>({ items, style, children }: { items: T[]; style?: Record<string, unknown>; children: (item: T, index: number) => unknown }) {
  return (
    <Box flexDirection="column" {...style}>
      {items.map((item, index) => children(item, index))}
    </Box>
  );
}
