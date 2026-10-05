import { Island } from "@snapfire/fsr-authoring/template";

import Week from "@src/ui/Week";

export default function Made({ kinds, totals, count, latest, past, spent, link }: { kinds: string[]; totals: Record<string, number>; count: number; latest: number; past: boolean; spent: number; link: string }) {
  const when = new Date(latest);
  return (
    <section className="made">
      <p className="kinds">{kinds.join(" ")} ({count})</p>
      <p className="totals">{Object.entries(totals).map(([kind, n]) => `${kind}=${n}`).join(" ")}</p>
      <p className="iso">{when.toISOString()}</p>
      <p className="day">{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeZone: "UTC" }).format(when)}</p>
      <p className="long">{when.toLocaleDateString(undefined, { dateStyle: "long" })}</p>
      <p className="spent">{new Intl.NumberFormat(undefined, { style: "currency", currency: "EUR", currencyDisplay: "code" }).format(spent)}</p>
      <p className="share">{new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(totals.fruit / count)}</p>
      <p className="past">{past ? "delivered" : "due"}</p>
      <a className="link" href={link}>
        again
      </a>
      <Island>
        <Week at={latest} />
      </Island>
    </section>
  );
}
