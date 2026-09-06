import type { CompilerProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function Compiler({ flags }: CompilerProps) {
  return (
    <div className="page">
      <section className="hero hero-small">
        <span className="hero-eyebrow">Supporting</span>
        <h1 className="hero-title">SnapFire Compiler</h1>
        <p className="hero-lede">
          Compile TypeScript for the browser. No Node.js required. It reads your sources, emits browser-native ES
          modules and stops if an import cannot be resolved. The binary is <code>snapfirec</code>.
        </p>
      </section>

      <section className="feature-grid">
        <div className="feature-card">
          <div className="card-icon">📦</div>
          <h3>No node_modules</h3>
          <p>
            Third-party libraries live in a committed vendor tree and resolve through a native browser import map.
            Nothing is fetched at build time that was not checked in.
          </p>
        </div>
        <div className="feature-card">
          <div className="card-icon">🗺️</div>
          <h3>The import map is the check</h3>
          <p>
            Pass <code>--import-map</code> and a bare import with no entry fails the build instead of failing in a
            browser tab.
          </p>
        </div>
        <div className="feature-card">
          <div className="card-icon">⚙️</div>
          <h3>One binary</h3>
          <p>
            Native and SWC-backed. There is no toolchain to install beside it and no configuration file it needs to
            start.
          </p>
        </div>
      </section>

      <section className="section-block">
        <div className="section-head">
          <h2>A build</h2>
          <p>What FSR runs for you, and what you run on its own.</p>
        </div>
        <div className="code-card">
          <div className="code-card-header">
            <span>sh</span>
          </div>
          <pre>
            <code>{`snapfirec --source-map --minify compact \\
  --public-path /static/js/app \\
  --import-map ../public/js/importmap.json`}</code>
          </pre>
        </div>
        <table className="bench-table">
          <thead>
            <tr>
              <th>Flag</th>
              <th>What it does</th>
            </tr>
          </thead>
          <tbody>
            {flags.map((f) => (
              <tr key={f.flag}>
                <td>
                  <code>{f.flag}</code>
                </td>
                <td>{f.does}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="section-block">
        <div className="section-head">
          <h2>Where it sits</h2>
        </div>
        <p>
          <Link href="/fsr">FSR</Link> uses the compiler to bundle the browser half of an application, but the dependency
          runs one way: the compiler knows nothing about FSR, plan files or routes. It is a TypeScript and CSS compiler
          you can point at any project, including one served by <Link href="/snapfire">snapfire</Link>.
        </p>
      </section>
    </div>
  );
}
