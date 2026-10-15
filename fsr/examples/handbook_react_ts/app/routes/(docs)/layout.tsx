import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="docs">
      <aside className="contents" aria-label="In the handbook">
        <p>In the handbook</p>
        <ol>
          <li><Link href="/install">Install</Link></li>
          <li><Link href="/faq">FAQ</Link></li>
          <li><Link href="/tips">Tips</Link></li>
        </ol>
      </aside>
      <div className="chapter">{children}</div>
    </div>
  );
}
