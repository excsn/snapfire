import { Link, type Children } from "@snapfire/fsr-authoring/template";

import type { Shed } from "@generated/services";

export default function ShedLayout({
  children,
  loans,
  weather,
  shed,
  reserved,
}: {
  children: Children;
  loans: Children;
  weather: Children;
  shed: Shed;
  reserved: number;
}) {
  return (
    <div className="shed">
      <header className="masthead">
        <div className="masthead-title">
          <h1>{shed.name}</h1>
          <p className="strap">{shed.strap}</p>
        </div>
        <nav className="masthead-nav">
          <Link href="/">Tools</Link>
          <Link href="/reserved">Reserved</Link>
        </nav>
        <shed-tally count={reserved}>
          <button type="button" className="tally" aria-label="reserved" aria-expanded="false">
            {reserved} reserved
          </button>
          <div className="tally-panel" hidden>
            <p>Reservations are kept in the session cookie. This count follows the store, which the layout seeds on a page and every fragment reseeds.</p>
          </div>
        </shed-tally>
      </header>
      <div className="columns">
        <main className="main">{children}</main>
        <aside className="side">
          {loans}
          {weather}
        </aside>
      </div>
      <footer className="colophon">
        The pages are lowered templates. The interactive pieces are custom elements the browser upgrades. The regions that talk to the server are htmx over fragments the host renders.
      </footer>
    </div>
  );
}
