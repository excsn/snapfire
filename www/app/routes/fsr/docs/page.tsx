import type { FsrDocsProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function DocsIndex({ chapters }: FsrDocsProps) {
  return (
    <div className="page">
      <header className="section-head">
        <h1>The FSR guide</h1>
        <p>
          Every chapter answers one question and reads in one sitting. Foundations first, then what you write, then what
          runs it.
        </p>
      </header>

      <ol className="chapter-index">
        {chapters.map((c) => (
          <li key={c.slug}>
            <Link href={`/fsr/docs/${c.slug}`} className="chapter-row">
              <span className="chapter-row-num">{c.number}</span>
              <span className="chapter-row-title">{c.title}</span>
              <span className="chapter-row-meta">
                {c.section} · {c.audience}
              </span>
            </Link>
          </li>
        ))}
      </ol>
    </div>
  );
}
