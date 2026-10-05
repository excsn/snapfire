import { useState } from "react";

export default function Picker({ names }: { names: string[] }) {
  const [picked, setPicked] = useState(0);
  const shown = names.slice(picked).concat(names.slice(0, picked));
  const label = shown.at(0)?.padStart(6, ".") ?? "";
  return (
    <div className="picker">
      <p className="shown">{shown.join(" > ")}</p>
      <p className="label">{label}</p>
      <p className="position">{names.indexOf(shown[0] ?? "") + 1} of {names.length}</p>
      <button className="next" onClick={() => setPicked((picked + 1) % names.length)}>
        next
      </button>
    </div>
  );
}
