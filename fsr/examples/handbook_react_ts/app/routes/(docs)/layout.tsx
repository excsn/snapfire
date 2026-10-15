import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";
import { chapters } from "@src/chapters";

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="docs">
      <aside className="contents" aria-label="In the handbook">
        <p>In the handbook</p>
        <ol>
          {chapters.map((chapter) => (
            <li key={chapter.href}><Link href={chapter.href}>{chapter.title}</Link></li>
          ))}
        </ol>
      </aside>
      <div className="chapter">{children}</div>
    </div>
  );
}
