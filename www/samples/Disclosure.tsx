import { useState } from "react";

export function Disclosure({ label }: { label: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button onClick={() => setOpen(!open)}>{open ? "Close" : "Open"}</button>
      {open ? <p>{label}</p> : null}
    </div>
  );
}
