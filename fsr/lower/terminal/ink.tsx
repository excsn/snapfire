// Ink's API for a terminal page, written over FSR's terminal primitives. The
// build reads this file wherever a page imports a name it exports from `ink`
// or `@snapfire/fsr/terminal`; every other name comes from the primitives.

import { useStore } from "@snapfire/fsr-client/react";

/** The terminal's size, which the terminal keeps in the store. */
export function useWindowSize() {
  const [columns] = useStore("$terminal.columns", 80);
  const [rows] = useStore("$terminal.rows", 24);
  return { columns, rows };
}
