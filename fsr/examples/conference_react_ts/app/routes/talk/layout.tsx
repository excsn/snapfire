import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function TalkLayout({ children }: { children: ReactNode }) {
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
