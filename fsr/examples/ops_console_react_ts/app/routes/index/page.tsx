import { island, Link } from "@snapfire/fsr-client/react";

import type { IndexProps } from "@generated/client";
import { TipList } from "@src/ui/Tips";

/** Nothing here is needed at first paint, so it hydrates when the main thread is idle. */
const Tips = island(TipList, { when: "idle" });

export default function Summary({ total, busy, regions }: IndexProps) {
  return (
    <div className="page summary">
      <h1>Fleet</h1>
      <p className="lede">
        {String(total)} agents, {String(busy)} with work queued.
      </p>
      <ul className="region-cards">
        {regions.map((r) => (
          <li key={r}>
            <Link href={`/agents?region=${r}`} className="region-card">
              {r}
            </Link>
          </li>
        ))}
      </ul>
      <p>
        <Link href="/agents">Every agent</Link> · <Link href="/help">How this works</Link> · <Link href="/state/one">Island state</Link>
      </p>
      <Tips />
    </div>
  );
}
