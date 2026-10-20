import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr/components";
import { chapters } from "@src/chapters";

export default function HandbookLayout({ children }: { children: ReactNode }) {
  return (
    <div className="handbook">
      <header className="masthead">
        <Link href="/" className="wordmark">
          FSR handbook
        </Link>
        <nav>
          {chapters.map((chapter) => (
            <Link key={chapter.href} href={chapter.href}>{chapter.title}</Link>
          ))}
        </nav>
      </header>
      <main>{children}</main>
      <footer>Every page here was written to a file before anyone asked for it.</footer>
    </div>
  );
}
