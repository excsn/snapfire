import type { ReactNode } from "react";
import type { Identity } from "@snapfire/fsr-authoring";

import { Header } from "@src/ui/Header";
import { LocalTime } from "@src/ui/LocalTime";

export default function Layout({ children, identity, csrf_token, theme, scheme }: { children: ReactNode; identity?: Identity; csrf_token?: string; theme: string; scheme: string }) {
  return (
    <>
      <Header identity={identity} csrfToken={csrf_token} theme={theme} scheme={scheme} />
      <main className="mx-auto max-w-[960px] px-6 pt-8 pb-12">{children}</main>
      <footer className="mx-auto flex max-w-[960px] items-start gap-3 px-6 pb-10 text-[13px] text-ink-soft">
        <span>One document, one session, one navigation. The billing pages under /billing are a site the portal mounts from its own artifact.</span>
        <LocalTime />
      </footer>
    </>
  );
}
