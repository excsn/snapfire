import type { ReactNode } from "react";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { who } from "@src/store";

const tab = "rounded-md px-2 py-1 text-indigo-700 hover:bg-indigo-50 aria-[current]:bg-indigo-600 aria-[current]:font-semibold aria-[current]:text-white";

/** The site's own layout, nested under the portal's when mounted and the whole page when the site runs alone. */
export default function BillingLayout({ children }: { children: ReactNode }) {
  const [name] = useStore(who, "");
  return (
    <section className="rounded-xl border border-indigo-100 bg-white px-6 py-5">
      <nav className="mb-4 flex items-center gap-3 border-b border-indigo-100 pb-3">
        <Link href="/billing" className={tab}>
          Invoices
        </Link>
        <Link href="/billing/overdue" className={tab}>
          Overdue
        </Link>
        <span className="ml-auto text-[13px] text-slate-500">{name ? `for ${name}` : "anonymous"}</span>
      </nav>
      {children}
    </section>
  );
}
