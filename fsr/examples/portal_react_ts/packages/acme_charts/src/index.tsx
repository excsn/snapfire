import type { ReactElement } from "react";

/** A number with a bar under it, `share` of the bar filled. */
export function Stat({ label, value, share }: { label: string; value: string; share: number }): ReactElement {
  return (
    <figure data-acme-stat="" className="m-0 grid gap-1">
      <figcaption className="text-xs uppercase tracking-widest text-fuchsia-700">{label}</figcaption>
      <b className="font-mono text-2xl tracking-tighter">{value}</b>
      <span className="block h-1.5 w-40 overflow-hidden rounded bg-fuchsia-100">
        <span className="block h-full bg-fuchsia-500" style={{ width: `${Math.round(share * 100)}%` }} />
      </span>
    </figure>
  );
}
