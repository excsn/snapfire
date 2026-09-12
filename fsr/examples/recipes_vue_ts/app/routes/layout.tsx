import type { ReactNode } from "react";
import { Island, Link } from "@snapfire/fsr-client/react";

import Tonight from "@src/ui/Tonight.vue";
import type { Box } from "@generated/services";

export default function BoxLayout({
  children,
  notes,
  market,
  box,
  planned,
}: {
  children: ReactNode;
  notes: ReactNode;
  market: ReactNode;
  box: Box;
  planned: number;
}) {
  return (
    <div className="box">
      <header className="masthead">
        <div className="masthead-title">
          <h1>{box.name}</h1>
          <p className="strap">{box.tagline}</p>
        </div>
        <nav className="masthead-nav">
          <Link href="/">Recipes</Link>
          <Link href="/tonight">Tonight</Link>
        </nav>
        <Island when="load">
          <Tonight count={planned} />
        </Island>
      </header>
      <div className="columns">
        <main className="main">{children}</main>
        <aside className="side">
          {notes}
          {market}
        </aside>
      </div>
      <footer className="colophon">
        The pages are lowered templates. Every interactive piece is a Vue single-file component, compiled by `snapfirec-vue`.
      </footer>
    </div>
  );
}
