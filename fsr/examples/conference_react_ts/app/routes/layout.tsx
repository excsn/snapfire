import type { ReactNode } from "react";
import { Island, Link } from "@snapfire/fsr-client/react";

import Saved from "@src/ui/Saved";
import type { Conference } from "@generated/services";

export default function ConferenceLayout({
  children,
  announcements,
  sponsors,
  conference,
  saved,
}: {
  children: ReactNode;
  announcements: ReactNode;
  sponsors: ReactNode;
  conference: Conference;
  saved: number;
}) {
  return (
    <div className="conference">
      <header className="masthead">
        <div className="masthead-title">
          <h1>{conference.name}</h1>
          <p className="strap">
            {conference.day} · {conference.venue}
          </p>
        </div>
        <nav className="masthead-nav">
          <Link href="/">Schedule</Link>
          <Link href="/saved">My schedule</Link>
        </nav>
        <Island when="load">
          <Saved count={saved} />
        </Island>
      </header>
      <div className="columns">
        <main className="main">{children}</main>
        <aside className="side">
          {announcements}
          {sponsors}
        </aside>
      </div>
      <footer className="colophon">
        No Rust in this application. `fsr dev app` builds it and the stock host serves it.
      </footer>
    </div>
  );
}
