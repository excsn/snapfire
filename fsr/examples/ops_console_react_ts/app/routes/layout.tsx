import type { ReactNode } from "react";
import { Slot } from "@snapfire/fsr-client/react";

import { Header } from "@src/ui/Header";

export default function Layout({ children, alerts }: { children: ReactNode; alerts: ReactNode }) {
  return (
    <>
      <Header />
      <div className="shell">
        <main className="shell-main">{children}</main>
        <aside className="shell-side">{alerts ?? <p className="quiet">Nothing is on fire.</p>}</aside>
      </div>
      <Slot name="drawer">
        <p className="drawer-hint">Settings open here.</p>
      </Slot>
    </>
  );
}
