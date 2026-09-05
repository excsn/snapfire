import type { ReactNode } from "react";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { who } from "@src/store";

/** The site's own layout, nested under the portal's when mounted and the whole page when the site runs alone. */
export default function BillingLayout({ children }: { children: ReactNode }) {
  const [name] = useStore(who, "");
  return (
    <section className="billing">
      <nav className="billing-nav">
        <Link href="/billing">Invoices</Link>
        <Link href="/billing/overdue">Overdue</Link>
        <span className="billing-who">{name ? `for ${name}` : "anonymous"}</span>
      </nav>
      {children}
    </section>
  );
}
