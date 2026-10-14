import { useState } from "react";

/** A count the reader raises, kept in the browser. */
export function Claps() {
  const [claps, setClaps] = useState(0);
  return (
    <button type="button" className="rounded-full border border-line px-3 py-1 text-sm" data-part="claps" onClick={() => setClaps(claps + 1)}>
      Clap {claps}
    </button>
  );
}
