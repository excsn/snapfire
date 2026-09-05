import type { ReactNode } from "react";
import { useState } from "react";
import { Link } from "@snapfire/fsr-client/react";

/** Local React state in a layout that two routes share. A navigation between them swaps the page region and keeps this island's DOM, so the count survives. */
export default function StateLayout({ children }: { children: ReactNode }) {
  const [count, setCount] = useState(0);
  return (
    <section className="page state">
      <h1>Island state across navigation</h1>
      <p className="lede">The counter lives in this layout. Click it, then move between the two routes beneath it: the page region is replaced and the layout, with its state, is kept.</p>
      <div className="state-bar">
        <button className="counter" onClick={() => setCount(count + 1)} aria-label="count">
          clicked {count}
        </button>
        <nav className="state-nav">
          <Link href="/state/one">route one</Link>
          <Link href="/state/two">route two</Link>
        </nav>
      </div>
      <div className="state-page">{children}</div>
    </section>
  );
}
