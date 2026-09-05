import type { ReactNode } from "react";
import { Slot } from "@snapfire/fsr-client/react";

import type { LayoutProps } from "@generated/client";
import { Header } from "@src/ui/Header";

export default function Layout({ cartCount, q, category, children, promo }: LayoutProps & { children: ReactNode; promo: ReactNode }) {
  return (
    <>
      <Header cartCount={cartCount} q={q} category={category} />
      {promo}
      {children}
      <Slot name="modal" />
    </>
  );
}
