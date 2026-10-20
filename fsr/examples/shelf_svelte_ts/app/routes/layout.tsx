import { Island, type Children } from "@snapfire/fsr-authoring/template";
import { Link } from "@snapfire/fsr/components";

import Tally from "@src/ui/Tally.svelte";
import Recent from "@src/ui/Recent.svelte";
import { books } from "@src/books";

export default function ShelfLayout({ children, done, total, reading }: { children: Children; done: number; total: number; reading: string }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          Shelf
        </Link>
        <Island when="load">
          <Tally done={done} total={total} />
        </Island>
        {reading === "" ? null : <span className="reading">Reading {reading}</span>}
        <Link href="/stats" className="to-stats">
          Stats
        </Link>
      </header>
      <div className="columns">
        <main className="content">{children}</main>
        <aside className="side">
          <Island when="idle">
            <Recent titles={books.map((book) => ({ id: book.id, title: book.title }))} />
          </Island>
        </aside>
      </div>
      <footer className="colophon">The pages are lowered templates. Every interactive piece is a Svelte component, compiled by snapfirec-svelte and mounted by Svelte in the browser.</footer>
    </div>
  );
}
