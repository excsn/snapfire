import { useState } from "react";

export default function AskDesk({ desk }: { desk: string }) {
  const [asked, setAsked] = useState(0);
  return (
    <p className="ask-desk">
      <button className="btn" onClick={() => setAsked(asked + 1)}>
        Ask the {desk} desk
      </button>
      {asked === 0 ? null : <span className="asked">Asked {asked} {asked === 1 ? "time" : "times"}</span>}
    </p>
  );
}
