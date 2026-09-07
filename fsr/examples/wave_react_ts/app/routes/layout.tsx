import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

import Name from "@src/ui/Name";

export default function WaveLayout({
  children,
  waves,
  rail,
  contacts,
  name,
}: {
  children: ReactNode;
  waves: ReactNode;
  rail: ReactNode;
  contacts: ReactNode;
  name?: string;
}) {
  return (
    <div className="app">
      <header>
        <Link href="/" className="wordmark">
          Waves
        </Link>
        <Name name={name ?? ""} />
      </header>
      <div className="panes">
        <aside className="side">
          {rail}
          {contacts}
        </aside>
        <aside className="inbox">{waves}</aside>
        <main className="open">{children}</main>
      </div>
    </div>
  );
}
