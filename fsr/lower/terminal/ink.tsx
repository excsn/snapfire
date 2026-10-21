// Ink's API for a terminal page, written over FSR's terminal primitives. The
// build reads this file wherever a page imports a name it exports from `ink`
// or `@snapfire/fsr/terminal`; every other name comes from the primitives.

import { action } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";

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
