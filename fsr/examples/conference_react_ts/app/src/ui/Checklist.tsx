import { useState } from "react";

const ITEMS = ["Badge", "Charger", "Coffee"];

export default function Checklist() {
  const [ticked, setTicked] = useState<string[]>([]);
  return (
    <fieldset className="checklist">
      <legend>Before you leave the hotel</legend>
      {ITEMS.map((item) => (
        <label key={item}>
          <input type="checkbox" checked={ticked.includes(item)} onChange={() => setTicked((now) => (now.includes(item) ? now.filter((t) => t !== item) : [...now, item]))} />
          {item}
        </label>
      ))}
      <p className="checklist-count">{ticked.length} of {ITEMS.length} packed</p>
    </fieldset>
  );
}
