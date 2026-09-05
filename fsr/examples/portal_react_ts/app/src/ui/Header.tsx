import type { Identity } from "@snapfire/fsr-authoring";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { teams, who } from "@src/store";

export function Header({ identity, csrfToken }: { identity?: Identity; csrfToken?: string }) {
  const [count] = useStore(teams, 0);
  const [name] = useStore(who, "");

  return (
    <header className="topbar">
      <Link href="/" className="brand">
        Acme
      </Link>
      <nav className="topnav">
        <Link href="/">Teams</Link>
        <Link href="/billing">Billing</Link>
        <Link href="/billing/overdue">Overdue</Link>
      </nav>
      <span className="pill" aria-label={`${count} teams`}>
        {count} teams
      </span>
      {identity ? (
        <form method="post" action="/auth/logout" className="who">
          <Link href="/account" className="pill pill-who">
            {name || identity.subject}
          </Link>
          <input type="hidden" name="_csrf" value={csrfToken ?? ""} />
          <button className="signout">Sign out</button>
        </form>
      ) : (
        <a href="/auth/login" className="pill pill-who" data-sf-native>
          Sign in
        </a>
      )}
    </header>
  );
}
