import type { ReactNode } from "react";
import type { Identity } from "@snapfire/fsr-authoring";

import { Header } from "@src/ui/Header";

export default function Layout({ children, identity, csrf_token }: { children: ReactNode; identity?: Identity; csrf_token?: string }) {
  return (
    <>
      <Header identity={identity} csrfToken={csrf_token} />
      <main className="portal-main">{children}</main>
      <footer className="portal-foot">One document, one session, one navigation. The billing pages under /billing are a site the portal mounts from its own artifact.</footer>
    </>
  );
}
