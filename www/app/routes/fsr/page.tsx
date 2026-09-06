import type { FsrProps } from "@generated/client";
import { Island, Link } from "@snapfire/fsr-client/react";
import { TelemetryBadge } from "@src/ui/TelemetryBadge";
import { IrInspector } from "@src/ui/IrInspector";
import { CrateLinks } from "@src/ui/CrateLinks";
import { FSR } from "@src/links";

export default function FsrOverview({ chapters }: FsrProps) {
  return (
    <div className="page home">
      <section className="hero">
        <span className="hero-eyebrow">SnapFire FSR</span>
        <h1 className="hero-title">
          Write TypeScript. <br />
          Run Rust. <br />
          Ditch Node.js.
        </h1>
        <p className="hero-lede">
          Loaders, actions and JSX are read at build time and lowered into an execution plan the host evaluates
          directly, so a page render is a walk over data rather than a call into a JavaScript engine. What that buys
          you: ~130µs renders, server islands, and bearer tokens your application code cannot reach.
        </p>

        <div className="hero-cta">
          <Link href="/fsr/docs" className="btn btn-primary">
            The guide, {chapters} chapters
          </Link>
          <CrateLinks github={FSR.github} crate={FSR.crate} className="btn btn-secondary" />
        </div>
      </section>

      {/* The Server Island Flex */}
      <section className="runtime-preview">
        <h2>Live Host Inspector</h2>
        <p>This widget below has <strong>zero client React code</strong> attached. Clicks round-trip to the Rust host and patch the DOM via diffing.</p>
        <Island when="visible" mode="server">
          <TelemetryBadge initialHost="Rust Stock Host (snapfire_fsr_host)" />
        </Island>
      </section>

      {/* Static Subtrees (Lowered into $h by FSR) */}
      <section className="feature-grid">
        <div className="feature-card">
          <div className="card-icon">⚡</div>
          <h3>~130µs Renders</h3>
          <p>Pages compile to a declarative IR. Rust evaluates the render tree directly without JS engine overhead.</p>
        </div>

        <div className="feature-card">
          <div className="card-icon">📦</div>
          <h3>Zero node_modules</h3>
          <p>Third-party libraries live in committed vendor trees and resolve through native browser import maps.</p>
        </div>

        <div className="feature-card">
          <div className="card-icon">🧩</div>
          <h3>Native Microfrontends</h3>
          <p>Mount standalone site artifacts under path prefixes with shared cookies, auth identity, and layouts.</p>
        </div>

        <div className="feature-card">
          <div className="card-icon">🛡️</div>
          <h3>Structural Token Custody</h3>
          <p>Application TypeScript code cannot see bearer tokens. Credentials live exclusively in protected host cells.</p>
        </div>
      </section>

      <section className="interactive-demo">
        <div className="section-head">
          <h2>See the Compiler in Action</h2>
          <p>TypeScript is parsed at build time into an executable JSON IR. Rust evaluates this without Node.</p>
        </div>

        <Island when="visible">
          <IrInspector />
        </Island>
      </section>
    </div>
  );
}