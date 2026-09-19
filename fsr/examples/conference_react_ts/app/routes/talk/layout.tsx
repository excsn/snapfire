import type { ReactNode } from "react";
import { Link, tree } from "@snapfire/fsr-client/react";

/** The talk renders inside this layout's React root: a click to another talk renders the new page from its props under the same crumbs. */
function TalkLayout({ children }: { children: ReactNode }) {
  return (
    <section className="talk-shell">
      <p className="crumbs">
        <Link href="/">The day</Link>
        <span className="sep">/</span>
        <span className="here">This talk</span>
      </p>
      {children}
    </section>
  );
}

export default tree(TalkLayout);
