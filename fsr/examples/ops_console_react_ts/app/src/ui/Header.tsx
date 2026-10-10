import type { Identity } from "@snapfire/fsr-authoring";
import { Link, useStore } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import { headline, openAlerts, region, selected, watching } from "@src/store";

export function Header({ identity, csrfToken }: { identity?: Identity; csrfToken?: string }) {
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
        <Link href="/agents" match="prefix" current="document">
          {t("header.agents")}
        </Link>
        <Link href="/help" prefetch="none" current="document">
          {t("header.help")}
        </Link>
      </nav>
      <p className="headline">{line}</p>
      <span className="pill pill-region">{shown === "all" ? t("agents.all") : shown}</span>
      <span className={open > 0 ? "pill pill-alert" : "pill"} aria-label={t("header.open_label", { count: open })}>
        {t("header.open", { count: open })}
      </span>
      <span className="pill" aria-label={t("agents.watching", { count: held })}>
        {t("header.watching", { count: held })}
      </span>
      {chosen ? <span className="pill pill-selected">#{chosen}</span> : null}
      {identity ? (
        <form method="post" action="/auth/logout" className="who">
          <Link href="/account" className="pill pill-who">
            {identity.subject}
          </Link>
          <input type="hidden" name="_csrf" value={csrfToken ?? ""} />
          <button className="signout">{t("header.sign_out")}</button>
        </form>
      ) : (
        <a href="/auth/login" data-sf-native className="pill pill-who signin">
          {t("header.sign_in")}
        </a>
      )}
      <Link href="/settings" into="drawer" className="gear" aria-label={t("header.settings")} title={t("header.settings")}>
        ⚙
      </Link>
    </header>
  );
}
