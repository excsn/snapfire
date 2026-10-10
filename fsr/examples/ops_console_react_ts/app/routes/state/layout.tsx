import type { ReactNode } from "react";
import { useState } from "react";
import { Link } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

/** Local React state in a layout that two routes share. A navigation between them swaps the page region and keeps this island's DOM, so the count survives. */
export default function StateLayout({ children }: { children: ReactNode }) {
  const [count, setCount] = useState(0);
  return (
    <section className="page state">
      <h1>{t("state.title")}</h1>
      <p className="lede">{t("state.lede")}</p>
      <div className="state-bar">
        <button className="counter" onClick={() => setCount(count + 1)} aria-label="count">
          {t("state.clicked", { count })}
        </button>
        <nav className="state-nav">
          <Link href="/state/one">{t("state.one")}</Link>
          <Link href="/state/two">{t("state.two")}</Link>
        </nav>
      </div>
      <div className="state-page">{children}</div>
    </section>
  );
}
