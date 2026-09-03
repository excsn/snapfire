import type { ReactNode } from "react";

import { Header } from "./Header";

export type HeaderProps = { cartCount: bigint | number; q?: string | null; category?: string | null };

export function Page({ header, className, children }: { header: HeaderProps; className: string; children: ReactNode }) {
  return (
    <>
      <Header {...header} />
      <main className={`page ${className}`}>{children}</main>
    </>
  );
}
