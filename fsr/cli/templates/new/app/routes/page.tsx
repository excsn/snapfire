import type { RootProps } from "@generated/client";

export default function Index({ greeting }: RootProps) {
  return (
    <section className="hero">
      <h1>{greeting}</h1>
      <p>This page was rendered by the host, with no JavaScript engine in the serving path.</p>
      <p>
        Edit <code>app/routes/page.tsx</code> and the browser reloads.
      </p>
    </section>
  );
}
