import { island, Link } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import type { RootProps } from "@generated/client";
import { TipList } from "@src/ui/Tips";

/** Nothing here is needed at first paint, so it hydrates when the main thread is idle. */
const Tips = island(TipList, { when: "idle" });

export default function Summary({ total, busy, regions }: RootProps) {
  return (
    <div className="page summary">
      <h1>{t("summary.title")}</h1>
      <p className="lede">
        {t("summary.lede", { total: String(total), busy: String(busy) })}
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
        <Link href="/agents">{t("summary.every")}</Link> · <Link href="/help">{t("summary.help")}</Link> · <Link href="/state/one">{t("summary.state")}</Link>
      </p>
      <Tips />
    </div>
  );
}
