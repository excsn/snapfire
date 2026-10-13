import type { ReactNode } from "react";
import type { Identity } from "@snapfire/fsr-authoring";

import { Header } from "@src/ui/Header";

export default function Layout({ children, identity, csrf_token }: { children: ReactNode; identity?: Identity; csrf_token?: string }) {
  return (
    <>
      <Header identity={identity} csrfToken={csrf_token} />
      <main className="mx-auto max-w-[960px] px-6 pt-8 pb-12">{children}</main>
      <footer className="mx-auto max-w-[960px] px-6 pb-10 text-[13px] text-ink-soft">One document, one session, one navigation. The billing pages under /billing are a site the portal mounts from its own artifact.</footer>
    </>
  );
}
