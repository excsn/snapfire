import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          {{name}}
        </Link>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
