import { useState } from "react";

export default function Badge({ left, kind }: { left: number; kind: string }) {
  const [taken, setTaken] = useState(0);
  const now = left - taken;
  if (now <= 0) {
    return <span className="badge out">out</span>;
  } else if (now < 3) {
    const word = now === 1 ? "one" : "two";
    return (
      <button className="badge low" onClick={() => setTaken(taken + 1)}>
        {word} left
      </button>
    );
  }
  switch (kind) {
    case "pantry":
      return <span className="badge plenty">plenty</span>;
    default:
      return (
        <button className="badge some" onClick={() => setTaken(taken + 1)}>
          {now} {kind}
        </button>
      );
  }
}
