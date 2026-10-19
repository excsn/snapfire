import { Island, Link } from "@snapfire/fsr-authoring/template";

import Stats from "@src/ui/Stats.svelte";
import type { StatsProps } from "@generated/client";

export default function StatsPage({ rows }: StatsProps) {
  return (
    <section className="stats-page">
      <p className="back">
        <Link href="/">All books</Link>
      </p>
      <Island when="load">
        <Stats rows={rows}>
          <p className="hint">Pages read, from the session.</p>
        </Stats>
      </Island>
    </section>
  );
}
