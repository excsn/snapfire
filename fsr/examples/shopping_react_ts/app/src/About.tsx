export default function About() {
  return (
    <main className="page about">
      <section className="prose">
        <h1>About this example</h1>
        <p>
          One binary runs two servers. The shopping service publishes an OpenAPI document; the FSR server imports it and
          never hand-writes a client.
        </p>
        <p>
          Routes come from <code>app/plan.json</code>. This one was added in Rust, which is what makes a route a binding
          rather than a fixed artifact.
        </p>
        <p>
          The loaders and actions are TypeScript, lowered at build time and run by the host. The pages are React islands
          hydrated in the browser; an action re-fetches the route so the cart count in the header follows every change.
        </p>
        <a className="btn btn-primary" href="/">
          Back to the catalog
        </a>
      </section>
    </main>
  );
}
