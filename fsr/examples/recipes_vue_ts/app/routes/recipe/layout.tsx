import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function RecipeLayout({ children }: { children: ReactNode }) {
  return (
    <section className="recipe-shell">
      <p className="crumbs">
        <Link href="/">The box</Link>
        <span className="sep">/</span>
        <span className="here">This recipe</span>
      </p>
      {children}
    </section>
  );
}
