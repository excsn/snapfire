//! What a terminal page and the terminal that runs it agree on past the host
//! vocabulary in `snapfire_fsr_ir::host`: Ink's API as provided modules and
//! the keys a page emits for the screen. Both sides build and read these
//! names only through the constants here.

/// Where a terminal page's Ink API is read from: [`INK_SOURCE`], provided to the build under this name.
pub const INK_FILE: &str = "$fsr/terminal.tsx";

/// Ink's hooks written over the terminal primitives, which the build reads wherever a page imports one of its exports from [`INK_MODULE`].
pub const INK_SOURCE: &str = include_str!("../terminal/ink.tsx");

/// The module a page imports FSR's Ink from: its elements, `useInput` and its hooks.
pub const INK_MODULE: &str = "@snapfire/fsr/terminal/ink";

/// The modules FSR provides beside Ink, each with the file the build provides under the specifier and that file's source: `(specifier, file, source)`.
pub const COMPANIONS: [(&str, &str, &str); 4] = [
  ("@snapfire/fsr/terminal/spinners", "$fsr/terminal/spinners.ts", include_str!("../terminal/spinners.ts")),
  ("@snapfire/fsr/terminal/spinner", "$fsr/terminal/spinner.tsx", include_str!("../terminal/spinner.tsx")),
  ("@snapfire/fsr/terminal/text-input", "$fsr/terminal/text-input.tsx", include_str!("../terminal/text-input.tsx")),
  ("@snapfire/fsr/terminal/big-text", "$fsr/terminal/big-text.tsx", include_str!("../terminal/big-text.tsx")),
];

/// The npm packages FSR provides its own of, each with the module of FSR's to import instead: `fsr migrate` writes the one for the other and the build refuses the npm name.
pub const REPLACED: [(&str, &str); 5] = [
  ("ink", INK_MODULE),
  ("cli-spinners", "@snapfire/fsr/terminal/spinners"),
  ("ink-spinner", "@snapfire/fsr/terminal/spinner"),
  ("ink-text-input", "@snapfire/fsr/terminal/text-input"),
  ("ink-big-text", "@snapfire/fsr/terminal/big-text"),
];

/// A focusable a terminal can move focus to, `{ id, autoFocus, isActive }`, one per `useFocus`, in render order.
pub const FOCUSABLE: &str = "focusable";

/// Where a terminal puts its cursor, `{ x, y }` from the corner of the output; null hides it. Ink's `setCursorPosition` sets it and the last one a step emits holds.
pub const CURSOR: &str = "cursor";

/// An effect whose dependencies change, `{ event, deps }`: the [`snapfire_fsr_ir::host::Event::Effect`] handler that runs it and the dependencies' values this render. A terminal runs the handler after a render whose values differ from the last ones it saw for that component, the first render included.
pub const EFFECT: &str = "effect";

/// The key a terminal primitive used as a value is held under, `{ "$primitive": name }`: `suspendTerminal` as Ink's `useApp` hands it back. A call through one is the primitive's.
pub const PRIMITIVE: &str = "$primitive";

/// The primitive a value refers to, read the way [`snapfire_fsr_ir::host::act_ref`] reads an act.
pub fn primitive_ref(expr: &snapfire_fsr_ir::Expr) -> Option<String> {
  use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
  match expr {
    Expr::Object(entries) => entries.iter().find_map(|entry| match entry {
      Entry::Field(key, Expr::Lit(Lit::Str(name))) if key == PRIMITIVE => Some(name.clone()),
      _ => None,
    }),
    Expr::Field(object, name) => match &**object {
      Expr::Object(entries) => entries.iter().rev().find_map(|entry| match entry {
        Entry::Field(key, value) if key == name => primitive_ref(value),
        _ => None,
      }),
      _ => None,
    },
    _ => None,
  }
}

