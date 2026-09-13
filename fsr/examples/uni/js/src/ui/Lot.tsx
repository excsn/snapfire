import { useState } from "react";

/**
 * The lot size a click changes. Placed in server mode, so no root is mounted
 * for it: each click posts to the host, Rust runs the handler, renders this
 * component again and the browser patches the markup it gets back.
 */
export default function Lot({ size }: { size: number }) {
  const [lot, setLot] = useState(size);
  return (
    <div className="lot">
      <span className="label">lot</span>
      <button type="button" onClick={() => setLot(lot - 10)}>
        &minus;
      </button>
      <output>{lot}</output>
      <button type="button" onClick={() => setLot(lot + 10)}>
        +
      </button>
      <span className="quiet">server mode: every click is a round trip, and nothing here mounted</span>
    </div>
  );
}
