import { useReducer } from "react";

import { useCounter, useToggle } from "@src/hooks/useCounter";

function tally(state: { n: number; log: string[] }, action: string) {
  return action === "add" ? { n: state.n + 1, log: [...state.log, "add"] } : { n: 0, log: [] };
}

export default function Counters() {
  const apples = useCounter(1);
  const { count: pears, doubled, bump } = useCounter(10, 5);
  const [open, toggle] = useToggle(false);
  const [state, dispatch] = useReducer(tally, { n: 3, log: [] });
  return (
    <div className="counters">
      <p className="apples">{apples.count} {apples.doubled}</p>
      <p className="pears">{pears} {doubled}</p>
      <p className="open">{open ? "open" : "shut"}</p>
      <p className="tally">{state.n} {state.log.join(",")}</p>
      <button className="pear" onClick={bump}>
        pears
      </button>
      <button className="toggle" onClick={toggle}>
        toggle
      </button>
      <button className="add" onClick={() => dispatch("add")}>
        add
      </button>
    </div>
  );
}
