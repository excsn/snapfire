export default function About() {
  return (
    <main className="about">
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
        <a href="/">back to the catalog</a>
      </p>
    </main>
  );
}
