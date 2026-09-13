import { useStore } from "@snapfire/fsr-client/react";

import { watchedKey } from "./Watch.js";

/** The day's move, and a click that makes this the watched symbol. */
export default function Chip({ symbol, change }: { symbol: string; change: number }) {
  const [held, setHeld] = useStore(watchedKey, "");
  const mine = held === symbol;
  return (
    <button type="button" className={`chip ${change < 0 ? "down" : "up"} ${mine ? "mine" : ""}`} onClick={() => setHeld(symbol)}>
      {change < 0 ? "" : "+"}
      {change.toFixed(1)}%
    </button>
  );
}
