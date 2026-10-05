import { useState } from "react";

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export default function Week({ at }: { at: number }) {
  const [ahead, setAhead] = useState(0);
  const day = new Date(at + ahead * 86_400_000);
  const seen = new Set([day.getUTCDay(), new Date(at).getUTCDay()]);
  return (
    <div className="week">
      <p className="when">{DAYS[day.getUTCDay()]} {day.toISOString().slice(0, 10)}</p>
      <p className="seen">{seen.size}</p>
      <button className="later" onClick={() => setAhead(ahead + 1)}>
        a day later
      </button>
    </div>
  );
}
