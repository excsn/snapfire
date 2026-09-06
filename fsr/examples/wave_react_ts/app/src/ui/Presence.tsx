import { useEffect, useState } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client/store";

import { join } from "@src/ui/wire";

/** Who is on the wave right now, over the participants the wave has ever had. Owns nothing: it takes a share of the page's one connection and lets it go when it unmounts. */
export default function Presence({ wave, participants }: { wave: string; participants: string[] }) {
  const [here] = useStore(key<string[]>("wave/here"), []);
  const [connected, setConnected] = useState(false);
  useEffect(() => join(`wave/${wave}`, setConnected), [wave]);
  return (
    <>
      <span className={connected ? "live on" : "live"} title={connected ? "following this wave" : "not connected"}>
        live
      </span>
      <ul className="people">
        {participants.map((who) => (
          <li key={who} className={here.includes(who) ? "person on" : "person"} title={here.includes(who) ? `${who} is here` : who}>
            {who}
          </li>
        ))}
      </ul>
    </>
  );
}
