import { useReducer } from "react";
import { Box, Static, Text, useCursor, useFocus, useFocusManager, useInput } from "ink";
import { stringWidth } from "@snapfire/fsr/terminal";

type Task = { id: number; title: string; done: boolean };
type Done = { id: number; title: string; at: string };
type State = { tasks: Task[]; log: Done[]; draft: string; next: number };
type Action =
  | { type: "type"; text: string }
  | { type: "erase" }
  | { type: "add" }
  | { type: "toggle"; id: number; at: string };

const prompt = "new task> ";

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "type":
      return { ...state, draft: state.draft + action.text };
    case "erase":
      return { ...state, draft: state.draft.slice(0, -1) };
    case "add": {
      const title = state.draft.trim();
      if (title === "") {
        return state;
      }
      return { ...state, tasks: [...state.tasks, { id: state.next, title, done: false }], draft: "", next: state.next + 1 };
    }
    case "toggle": {
      const task = state.tasks.find((t) => t.id === action.id);
      if (!task) {
        return state;
      }
      const log = task.done ? state.log : [...state.log, { id: state.log.length, title: task.title, at: action.at }];
      return { ...state, tasks: state.tasks.map((t) => (t.id === action.id ? { ...t, done: !t.done } : t)), log };
    }
  }
}

function Row({ task }: { task: Task }) {
  const { isFocused } = useFocus({ id: `task-${task.id}` });
  return (
    <Box>
      <Box width="10%">
        <Text color={isFocused ? "cyan" : undefined}>{isFocused ? ">" : " "}</Text>
      </Box>
      <Box width="70%">
        <Text strikethrough={task.done} dimColor={task.done}>
          {task.title}
        </Text>
      </Box>
      <Box width="20%">
        <Text color={task.done ? "green" : "yellow"}>{task.done ? "done" : "open"}</Text>
      </Box>
    </Box>
  );
}

export default function Tasks() {
  const [state, dispatch] = useReducer(reducer, { tasks: [], log: [], draft: "", next: 1 });
  const { activeId } = useFocusManager();
  const { setCursorPosition } = useCursor();

  useInput((input, key) => {
    if (key.return) {
      dispatch({ type: "add" });
    } else if (key.backspace || key.delete) {
      dispatch({ type: "erase" });
    } else if (input === " " && activeId) {
      dispatch({ type: "toggle", id: Number(activeId.slice("task-".length)), at: new Date().toLocaleTimeString() });
    } else if (input !== "" && !key.ctrl && !key.meta && !key.tab) {
      dispatch({ type: "type", text: input });
    }
  });

  const open = state.tasks.filter((t) => !t.done).length;
  setCursorPosition({ x: stringWidth(prompt + state.draft), y: 2 });

  return (
    <>
      <Static items={state.log}>
        {(done) => (
          <Text key={done.id} color="green">
            ✔ {done.title} at {done.at}
          </Text>
        )}
      </Static>
      <Box flexDirection="column">
        <Text bold>
          Tasks, {open} open of {state.tasks.length}
        </Text>
        <Text dimColor>Type a task and press Enter. Tab moves between tasks, space marks one done.</Text>
        <Text>
          {prompt}
          {state.draft}
        </Text>
        <Box flexDirection="column" marginTop={1}>
          {state.tasks.map((task) => (
            <Row key={task.id} task={task} />
          ))}
        </Box>
      </Box>
    </>
  );
}
