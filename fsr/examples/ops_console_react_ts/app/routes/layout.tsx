import type { ReactNode } from "react";
import type { Identity } from "@snapfire/fsr-authoring";
import { Slot } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import { Header } from "@src/ui/Header";

export default function Layout({ children, alerts, identity, csrf_token }: { children: ReactNode; alerts: ReactNode; identity?: Identity; csrf_token?: string }) {
  return (
    <>
      <Header identity={identity} csrfToken={csrf_token} />
      <div className="shell">
        <main className="shell-main">{children}</main>
        <aside className="shell-side">{alerts}</aside>
      </div>
      <Slot name="drawer">
        <p className="drawer-hint">{t("layout.drawer_hint")}</p>
      </Slot>
    </>
  );
}
