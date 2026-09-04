import type { ReactNode } from "react";

import type { LayoutProps } from "@generated/client";
import { Header } from "@src/ui/Header";

export default function Layout({ cartCount, q, category, children }: LayoutProps & { children: ReactNode }) {
  return (
    <>
      <Header cartCount={cartCount} q={q} category={category} />
      {children}
    </>
  );
}
