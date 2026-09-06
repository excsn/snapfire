import type { FsrDocsSlugProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";
import { Prose } from "@src/ui/Prose";

export default function DocsChapter({ chapter, prev, next, allChapters }: FsrDocsSlugProps) {
  return (
    <div className="page docs-layout">
      <aside className="docs-sidebar">
        <div className="sidebar-header">
          <Link href="/fsr/docs" className="sidebar-title">
            The FSR guide
          </Link>
        </div>
        <nav className="sidebar-nav">
          {allChapters.map((c) => (
            <Link
              key={c.slug}
              href={`/fsr/docs/${c.slug}`}
              className={c.slug === chapter.slug ? "nav-item nav-item-active" : "nav-item"}
            >
              <span className="nav-num">{c.number}</span>
              <span className="nav-label">{c.title}</span>
            </Link>
          ))}
        </nav>
      </aside>

      <article className="docs-content">
        <header className="chapter-header">
          <span className="chapter-meta">
            {chapter.section} · for {chapter.audience}
          </span>
          <h1 className="chapter-title">
            {chapter.number}. {chapter.title}
          </h1>
        </header>

        <Prose blocks={chapter.blocks} />

        <nav className="chapter-pagination">
          {prev ? (
            <Link href={`/fsr/docs/${prev.slug}`} className="pagination-btn prev">
              <span className="pagination-dir">Previous</span>
              <span className="pagination-title">{prev.title}</span>
            </Link>
          ) : (
            <div />
          )}
          {next ? (
            <Link href={`/fsr/docs/${next.slug}`} className="pagination-btn next">
              <span className="pagination-dir">Next</span>
              <span className="pagination-title">{next.title}</span>
            </Link>
          ) : (
            <div />
          )}
        </nav>
      </article>
    </div>
  );
}
