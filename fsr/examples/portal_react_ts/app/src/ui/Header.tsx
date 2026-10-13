import type { Identity } from "@snapfire/fsr-authoring";
import { Link, useStore } from "@snapfire/fsr-client/react";

import { teams, who } from "@src/store";
import { LookControls } from "@src/ui/LookControls";

const tab = "rounded-md px-2.5 py-1.5 text-ink-soft hover:bg-accent-soft hover:text-accent aria-[current]:bg-accent-soft aria-[current]:text-accent aria-[current=page]:font-semibold";

export function Header({ identity, csrfToken, theme, scheme }: { identity?: Identity; csrfToken?: string; theme: string; scheme: string }) {
  const [count] = useStore(teams, 0);
  const [name] = useStore(who, "");

  return (
    <header className="sticky top-0 z-10 flex min-h-[54px] flex-wrap items-center gap-x-[18px] gap-y-2 border-b border-line bg-card px-6 py-2">
      <Link href="/" className="font-bold text-ink">
        Acme
      </Link>
      <nav className="flex gap-1">
        <Link href="/" className={tab}>
          Teams
        </Link>
        <Link href="/billing" match="prefix" className={tab}>
          Billing
        </Link>
        <Link href="/billing/overdue" className={tab}>
          Overdue
        </Link>
        <Link href="/blog" match="prefix" className={tab}>
          Blog
        </Link>
        <Link href="/status" match="prefix" className={tab}>
          Status
        </Link>
        <Link href="/reports" match="prefix" className={tab}>
          Reports
        </Link>
      </nav>
      <span className="pill" aria-label={`${count} teams`}>
        {count} teams
      </span>
      <LookControls theme={theme} scheme={scheme} />
      {identity ? (
        <form method="post" action="/auth/logout" className="ml-auto flex items-center gap-2">
          <Link href="/account" className="pill font-medium text-ink">
            {name || identity.subject}
          </Link>
          <input type="hidden" name="_csrf" value={csrfToken ?? ""} />
          <button className="button">Sign out</button>
        </form>
      ) : (
        <a href="/auth/login" className="pill ml-auto font-medium text-ink" data-sf-native>
          Sign in
        </a>
      )}
    </header>
  );
}
