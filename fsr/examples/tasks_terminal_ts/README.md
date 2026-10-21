# tasks_terminal_ts

A task list in the terminal, written against Ink's API and run by `fsr dev --terminal` with no Node. It exercises what a real Ink tool leans on: a reducer holding the list, a prompt whose cursor `useCursor` places with `string-width`, rows focusable with `useFocus` that Tab moves through, `useFocusManager().activeId` naming the focused one, Ink's `Static` printing each finished task above the page with the local time, and columns sized in percentages.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/tasks_terminal_ts
fsr dev app --terminal
```

Type a task and press Enter to add it. Tab and Shift+Tab move between tasks, space marks the focused one done and Ctrl-C quits. `fsr/cli/tests/terminal_examples.rs` drives the same keys and checks each frame.
