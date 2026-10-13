import type { ReactNode } from "react";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { who } from "@src/store";

/** The site's own layout: nested under the portal's when the portal renders a page from this process, the whole page when the site runs alone. */
export default function ReportsLayout({ children }: { children: ReactNode }) {
  const [name] = useStore(who, "");
  return (
    <section className="rounded-xl bg-slate-900 px-6 py-5 text-slate-200 [&_h1]:text-white">
      <nav className="mb-4 flex items-center gap-3 border-b border-slate-700 pb-3">
        <Link href="/reports" className="rounded-md px-2 py-1 text-sky-400 aria-[current]:bg-sky-500 aria-[current]:font-semibold aria-[current]:text-slate-950">
          Reports
        </Link>
        <span className="ml-auto text-[13px] text-slate-400">{name ? `for ${name}` : "anonymous"}</span>
      </nav>
      {children}
    </section>
  );
}
