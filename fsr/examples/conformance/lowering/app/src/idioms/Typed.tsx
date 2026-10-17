import { useState } from "react";

export default function Typed({ map, set, date, big }: { map: Map<string, number>; set: Set<string>; date: Date; big: bigint }) {
  const [shown, setShown] = useState("");
  return (
    <p className="typed">
      <button onClick={() => setShown(`${map.get("bob")} ${set.has("next")} ${date.getUTCFullYear()} ${typeof big}`)}>look</button>
      {map instanceof Map ? "map" : "no map"} {set instanceof Set ? "set" : "no set"} {date instanceof Date ? "date" : "no date"} {typeof big === "bigint" ? "bigint" : "no bigint"} {map.size} {date.toISOString()} {shown}
    </p>
  );
}
