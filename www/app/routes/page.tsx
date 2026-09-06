import type { IndexProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function Home({ chapters }: IndexProps) {
  return (
    <div className="page home">
      <section className="hero">
        <span className="hero-eyebrow">The flagship</span>
        <h1 className="hero-title">
          Write TypeScript. <br />
          Run Rust. <br />
          Ditch Node.js.
        </h1>
        <p className="hero-lede">
          SnapFire FSR lowers TypeScript loaders, actions and JSX into an execution plan evaluated natively in Rust at
          memory speed. No V8 isolates, no cold starts, no <code>node_modules</code>: pure native SSR with clean React
          hydration.
        </p>
        <div className="hero-cta">
          <Link href="/fsr" className="btn btn-primary">
            What FSR is
          </Link>
          <Link href="/fsr/docs" className="btn btn-secondary">
            The guide, {chapters} chapters
          </Link>
        </div>
      </section>

      <section className="product-grid">
        <article className="product-card product-card-flagship">
          <span className="product-tag">Flagship</span>
          <h2>
            <Link href="/fsr">SnapFire FSR</Link>
          </h2>
          <p className="product-what">A full-stack runtime.</p>
          <p>
            The application is TypeScript under <code>app/</code>. The runtime is Rust. Loaders and actions are read at
            build time and become data the host executes directly, so nothing in the serving path is a JavaScript
            engine.
          </p>
          <ul className="product-points">
            <li>Native SSR of React pages with no JS engine</li>
            <li>Typed service calls generated from OpenAPI and Protobuf</li>
            <li>Server islands, sessions and identity in the host</li>
          </ul>
          <Link href="/fsr" className="product-more">
            Read on
          </Link>
        </article>

        <article className="product-card">
          <span className="product-tag">Supporting</span>
          <h2>
            <Link href="/compiler">SnapFire Compiler</Link>
          </h2>
          <p className="product-what">A TypeScript and CSS compiler for the browser.</p>
          <p>
            Compiles TypeScript to browser-native ES modules with no Node and no <code>node_modules</code>. FSR uses
            it to bundle the client half; it knows nothing about FSR and works on its own. Its binary is{" "}
            <code>snapfirec</code>.
          </p>
          <ul className="product-points">
            <li>Native, SWC-backed, no toolchain to install</li>
            <li>Source maps, minification, import maps</li>
            <li>A preload manifest for what the page needs first</li>
          </ul>
          <Link href="/compiler" className="product-more">
            Read on
          </Link>
        </article>

        <article className="product-card">
          <span className="product-tag">Where it started</span>
          <h2>
            <Link href="/snapfire">snapfire</Link>
          </h2>
          <p className="product-what">Tera templates over Actix, with live reload.</p>
          <p>
            The original crate: a templating library with an integrated zero-configuration reload server. Edit a
            template, the browser updates. Every development-only feature compiles out of a release build.
          </p>
          <ul className="product-points">
            <li>First-class Tera 2 and Actix Web integration</li>
            <li>Live reload with no configuration</li>
            <li>Nothing development-only in a release binary</li>
          </ul>
          <Link href="/snapfire" className="product-more">
            Read on
          </Link>
        </article>
      </section>
    </div>
  );
}
