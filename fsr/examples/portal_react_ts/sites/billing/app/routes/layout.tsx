import type { ReactNode } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { Link } from "@snapfire/fsr/components";

import { who } from "@src/store";

const tab = "rounded-md px-2 py-1 text-accent hover:bg-accent-soft aria-[current]:bg-accent aria-[current]:font-semibold aria-[current]:text-white";

export const theme = { name: "ledger", force: true };

/** The site's own layout, nested under the portal's when mounted and the whole page when the site runs alone. */
export default function BillingLayout({ children }: { children: ReactNode }) {
  const [name] = useStore(who, "");
  return (
    <section className="rounded-xl border border-line bg-card px-6 py-5">
      <nav className="mb-4 flex items-center gap-3 border-b border-line pb-3">
        <Link href="/billing" className={tab}>
          Invoices
        </Link>
        <Link href="/billing/overdue" className={tab}>
          Overdue
        </Link>
        <span className="ml-auto text-[13px] text-ink-soft">{name ? `for ${name}` : "anonymous"}</span>
      </nav>
      <p className="mb-4 text-[13px] text-ink-soft">Billing keeps its own ledger theme. The theme you pick applies on the other sites; light or dark applies here too.</p>
      {children}
    </section>
  );
}
