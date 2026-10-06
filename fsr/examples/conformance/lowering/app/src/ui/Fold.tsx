import { useState, type ReactNode } from "react";

export default function Fold({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="fold">
      <button className="fold-toggle" onClick={() => setOpen(!open)}>
        {title}
      </button>
      {open && <div className="fold-body">{children}</div>}
    </div>
  );
}
