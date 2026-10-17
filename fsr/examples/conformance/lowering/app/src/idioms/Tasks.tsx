import { createContext, useContext, useReducer } from "react";

interface Task {
  id: number;
  text: string;
  done: boolean;
}

const Dispatch = createContext<((action: { type: string; id: number }) => void) | null>(null);

function tasksReducer(tasks: Task[], action: { type: string; id: number }) {
  switch (action.type) {
    case "toggled":
      return tasks.map((t) => (t.id === action.id ? { ...t, done: !t.done } : t));
    default:
      throw Error("Unknown action: " + action.type);
  }
}

function Row({ task }: { task: Task }) {
  const dispatch = useContext(Dispatch);
  return (
    <li className={task.done ? "done" : "open"}>
      <button onClick={() => dispatch?.({ type: "toggled", id: task.id })}>{task.text}</button>
    </li>
  );
}

function Count({ tasks, dispatch }: { tasks: Task[]; dispatch: unknown }) {
  const open: string[] = [];
  tasks.forEach((task) => {
    if (task.done) {
      return;
    }
    open.push(task.text);
  });
  return <p className="open">{open.length} open{dispatch ? "" : "!"}</p>;
}

function Left({ tasks }: { tasks: Task[] }) {
  const left = tasks.filter((task) => !task.done);
  return <p className="left">{left.length} left</p>;
}

export default function Tasks() {
  const [tasks, dispatch] = useReducer(tasksReducer, [
    { id: 0, text: "Plan", done: false },
    { id: 1, text: "Build", done: true },
  ]);
  return (
    <Dispatch value={dispatch}>
      <ul className="tasks">
        {tasks.map((task) => (
          <Row key={task.id} task={task} />
        ))}
      </ul>
      <Count tasks={tasks} dispatch={dispatch} />
      <Left tasks={tasks} />
    </Dispatch>
  );
}
