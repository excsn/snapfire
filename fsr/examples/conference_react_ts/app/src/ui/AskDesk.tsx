import { useState } from "react";
import { useStore } from "@snapfire/fsr/client/react";

import { noticeCount, packed } from "@src/store";

export default function AskDesk({ desk }: { desk: string }) {
  const [asked, setAsked] = useState(0);
  const [items] = useStore(packed, []);
  const [notices] = useStore(noticeCount, 0);
  return (
    <p className="ask-desk">
      <button className="btn" onClick={() => setAsked(asked + 1)}>
        Ask the {desk} desk
      </button>
      <span className="desk-packed">The desk sees {items.length} packed</span>
      <span className="desk-notices">{notices} notices posted</span>
      {asked === 0 ? null : <span className="asked">Asked {asked} {asked === 1 ? "time" : "times"}</span>}
    </p>
  );
}
