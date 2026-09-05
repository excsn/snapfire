import { Link, useStore } from "@snapfire/fsr-client/react";

import { headline, openAlerts, region, selected, watching } from "@src/store";

export function Header() {
  const [open] = useStore(openAlerts, 0);
  const [held] = useStore(watching, 0);
  const [shown] = useStore(region, "all");
  const [chosen] = useStore(selected, "");
  const [line] = useStore(headline, "");

  return (
    <header className="topbar">
      <Link href="/" className="brand">
        ops console
      </Link>
      <nav className="topnav">
        <Link href="/agents">Agents</Link>
        <Link href="/help" prefetch="none">
          Help
        </Link>
      </nav>
      <p className="headline">{line}</p>
      <span className="pill pill-region">{shown}</span>
      <span className={open > 0 ? "pill pill-alert" : "pill"} aria-label={`${open} open alerts`}>
        {open} open
      </span>
      <span className="pill" aria-label={`watching ${held} agents`}>
        watching {held}
      </span>
      {chosen ? <span className="pill pill-selected">#{chosen}</span> : null}
      <Link href="/settings" into="drawer" className="gear" aria-label="Settings" title="Settings">
        ⚙
      </Link>
    </header>
  );
}
