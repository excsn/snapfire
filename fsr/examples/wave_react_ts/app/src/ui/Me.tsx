import { useState } from "react";
import type { KeyboardEvent, MouseEvent } from "react";

import Name from "@src/ui/Name";

/** The reader's name at the foot of the side pane, which opens their settings: for now the name itself, changed through the same form that set it. Escape, the Close button or a click outside the sheet closes it. */
export default function Me({ name }: { name: string }) {
  const [open, setOpen] = useState(false);

  function outside(event: MouseEvent<HTMLDivElement>): void {
    if (event.target === event.currentTarget) setOpen(false);
  }

  function escape(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === "Escape") setOpen(false);
  }

  return (
    <>
      <button type="button" className="me" title="your settings" onClick={() => setOpen(true)}>
        {name}
      </button>
      {open ? (
        <div className="modal" onClick={outside} onKeyDown={escape}>
          <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="settings-title">
            <h2 id="settings-title">Settings</h2>
            <label>Your name on every wave</label>
            <Name name={name} />
            <button type="button" className="close" onClick={() => setOpen(false)}>
              Close
            </button>
          </div>
        </div>
      ) : null}
    </>
  );
}
