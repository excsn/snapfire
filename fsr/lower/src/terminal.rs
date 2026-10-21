//! What a terminal page and the terminal that runs it agree on: the kinds of value a body emits.

/// A focusable a terminal can move focus to, `{ id, autoFocus, isActive }`, one per `useFocus`, in render order.
pub const FOCUSABLE: &str = "focusable";

/// Where a terminal puts its cursor, `{ x, y }` from the corner of the output; null hides it. Ink's `setCursorPosition` sets it and the last one a step emits holds.
pub const CURSOR: &str = "cursor";

/// The body `let` that emits a value of `kind`.
pub fn emit(kind: &str) -> String {
  format!("{}{kind}", snapfire_fsr_ir::render::EMIT_PREFIX)
}
