import type { ReactNode } from "react";
import { useState } from "react";
import { Link } from "@snapfire/fsr/components";

/** Registered by hand in `main.ts`, so this whole segment mounts with no patcher. */
export default function DeskLayout({ track, children }: { track: string; children: ReactNode }) {
  const [asked, setAsked] = useState(0);
  return (
    <section className="page desk">
      <h2 className="desk-track">Desk for {track}</h2>
      <button className="desk-ask" onClick={() => setAsked(asked + 1)}>
        Asked {asked}
      </button>
      <nav className="desk-tracks">
        <Link href="/desk">All</Link>
        <Link href="/desk?track=Data">Data</Link>
      </nav>
      {children}
    </section>
  );
}
