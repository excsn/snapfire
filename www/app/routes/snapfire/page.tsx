import type { SnapfireProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function Snapfire({ version }: SnapfireProps) {
  return (
    <div className="page">
      <section className="hero hero-small">
        <span className="hero-eyebrow">Where it started · v{version}</span>
        <h1 className="hero-title">snapfire</h1>
        <p className="hero-lede">
          A high-productivity templating library for Rust with an integrated, zero-configuration live-reload server.
          First-class Tera 2 and Actix Web integration.
        </p>
      </section>

      <section className="feature-grid">
        <div className="feature-card">
          <div className="card-icon">🔁</div>
          <h3>Reload with no configuration</h3>
          <p>
            Edit a template or a static asset and the browser updates. The reload server is part of the library rather
            than something you wire up beside it.
          </p>
        </div>
        <div className="feature-card">
          <div className="card-icon">🪶</div>
          <h3>Nothing left in release</h3>
          <p>
            Every development-only feature compiles out. A release binary carries no watcher, no socket and no reload
            script.
          </p>
        </div>
        <div className="feature-card">
          <div className="card-icon">🧱</div>
          <h3>Tera 2 and Actix</h3>
          <p>
            Not a framework of its own: it makes the two crates you already use pleasant to develop against, and gets
            out of the way in production.
          </p>
        </div>
      </section>

      <section className="section-block">
        <div className="section-head">
          <h2>Which one do I want?</h2>
        </div>
        <p>
          Reach for snapfire when the server renders HTML from Tera templates and you want the edit-refresh loop to be
          instant. Reach for <Link href="/fsr">FSR</Link> when the application is a TypeScript one, the pages are React
          and you want the loaders and actions to run in Rust rather than in Node.
        </p>
        <p>
          They share a compiler: <Link href="/compiler">SnapFire Compiler</Link> builds the browser assets either way.
        </p>
      </section>
    </div>
  );
}
