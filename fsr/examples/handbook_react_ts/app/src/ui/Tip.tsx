import { useState } from "react";
import TipBody from "../../content/tip.mdx";

/** A tip that opens on a click, its text an `.mdx` file the browser renders. */
export function Tip() {
  const [open, setOpen] = useState(false);
  return (
    <div className="tip" data-part="tip">
      <button type="button" onClick={() => setOpen(!open)}>
        {open ? "Hide the tip" : "Show the tip"}
      </button>
      {open ? <TipBody /> : null}
    </div>
  );
}
