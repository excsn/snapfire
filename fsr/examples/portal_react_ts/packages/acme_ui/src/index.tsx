import type { ReactElement } from "react";

import { cn } from "./cn";

export type Tone = "violet" | "teal";

const tones: Record<Tone, string> = {
  violet: "bg-violet-100 text-violet-800 ring-violet-600/20",
  teal: "bg-teal-100 text-teal-800 ring-teal-600/20",
};

/** A short label in a rounded pill. */
export function Badge({ tone = "violet", dot = false, children }: { tone?: Tone; dot?: boolean; children: string }): ReactElement {
  return (
    <span data-acme-badge={tone} className={cn("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset", tones[tone])}>
      {dot && <span className={cn("size-1.5 rounded-full", tone === "violet" ? "bg-violet-500" : "bg-teal-500")} />}
      {children}
    </span>
  );
}
