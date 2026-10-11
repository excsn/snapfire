import type { ReactNode } from "react";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { who } from "@src/store";

/** The site's own layout: nested under the portal's when the portal renders a page from this process, the whole page when the site runs alone. */
export default function ReportsLayout({ children }: { children: ReactNode }) {
  const [name] = useStore(who, "");
  return (
    <section className="reports">
      <nav className="reports-nav">
        <Link href="/reports">Reports</Link>
        <span className="reports-who">{name ? `for ${name}` : "anonymous"}</span>
      </nav>
      {children}
    </section>
  );
}
