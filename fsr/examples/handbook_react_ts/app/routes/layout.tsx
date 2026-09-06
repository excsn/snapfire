import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function HandbookLayout({ children }: { children: ReactNode }) {
  return (
    <div className="handbook">
      <header className="masthead">
        <Link href="/" className="wordmark">
          FSR handbook
        </Link>
        <nav>
          <Link href="/install">Install</Link>
          <Link href="/faq">FAQ</Link>
        </nav>
      </header>
      <main>{children}</main>
      <footer>Every page here was written to a file before anyone asked for it.</footer>
    </div>
  );
}
