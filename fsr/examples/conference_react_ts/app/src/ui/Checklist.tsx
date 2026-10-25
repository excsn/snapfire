import { useStore } from "@snapfire/fsr/client/react";

import { packed } from "@src/store";
import "./checklist.css";

const ITEMS = ["Badge", "Charger", "Coffee"];

export default function Checklist() {
  const [ticked, setTicked] = useStore(packed, []);
  return (
    <fieldset className="checklist">
      <legend>Before you leave the hotel</legend>
      {ITEMS.map((item) => (
        <label key={item}>
          <input type="checkbox" checked={ticked.includes(item)} onChange={() => setTicked(ticked.includes(item) ? ticked.filter((t) => t !== item) : [...ticked, item])} />
          {item}
        </label>
      ))}
      <p className="checklist-count">{ticked.length} of {ITEMS.length} packed</p>
    </fieldset>
  );
}
