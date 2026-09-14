import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

import Me from "@src/ui/Me";
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
      {name ? null : (
        <div className="naming">
          <p>Name yourself to write on a wave.</p>
          <Name name="" />
        </div>
      )}
      <div className="panes">
        <aside className="side">
          {rail}
          {contacts}
          <footer className="side-foot">
            <Link href="/" className="wordmark">
              Waves
            </Link>
            {name ? <Me name={name} /> : null}
          </footer>
        </aside>
        <aside className="inbox">{waves}</aside>
        <main className="open">{children}</main>
      </div>
    </div>
  );
}
