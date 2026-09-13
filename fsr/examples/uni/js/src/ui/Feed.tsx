import { useState } from "react";
import { useStore } from "@snapfire/fsr-client/react";

import { watchedKey } from "./Watch.js";

interface Headline {
  symbol: string;
  text: string;
}

export default function Feed({ headlines }: { headlines: Headline[] }) {
  const [held] = useStore(watchedKey, "");
  const [mine, setMine] = useState(false);
  const shown = mine ? headlines.filter((h) => h.symbol === held) : headlines;
  return (
    <div className="feed">
      <label className="only">
        <input type="checkbox" checked={mine} onChange={() => setMine(!mine)} /> only {held || "the watched symbol"}
      </label>
      <ol>
        {shown.map((h) => (
          <li key={h.text}>
            <span className="sym">{h.symbol}</span>
            {h.text}
          </li>
        ))}
      </ol>
      {shown.length === 0 ? <p className="quiet">Nothing for {held} today.</p> : null}
    </div>
  );
}
