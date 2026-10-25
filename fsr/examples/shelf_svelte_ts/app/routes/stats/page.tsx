import { Island } from "@snapfire/fsr/components";
import { Link } from "@snapfire/fsr/components";

import Figure from "@src/ui/Figure.svelte";
import Stats from "@src/ui/Stats.svelte";
import type { StatsProps } from "@generated/client";

export default function StatsPage({ rows }: StatsProps) {
  return (
    <section className="stats-page">
      <p className="back">
        <Link href="/">All books</Link>
      </p>
      <Island when="load">
        <Figure caption="pages read per book" />
      </Island>
      <Island when="load">
        <Figure caption="books finished per month" />
      </Island>
      <Island when="load">
        <Stats rows={rows}>
          <p className="hint">Pages read, from the session.</p>
        </Stats>
      </Island>
    </section>
  );
}
