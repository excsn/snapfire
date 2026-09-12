import { useState } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { Link } from "@snapfire/fsr-client/react";

import { savedCount } from "@src/store";

export default function Saved({ count }: { count: number }) {
  const [held] = useStore(savedCount, count);
  const [open, setOpen] = useState(false);
  return (
    <div className={open ? "saved saved-open" : "saved"}>
      <button className="saved-count" onClick={() => setOpen(!open)} aria-label="my schedule">
        {held} saved
      </button>
      {open ? (
        <p className="saved-note">
          Kept in the session cookie. <Link href="/saved">See them</Link>. This panel is React state in the root layout: move between
          routes and it stays open.
        </p>
      ) : null}
    </div>
  );
}
