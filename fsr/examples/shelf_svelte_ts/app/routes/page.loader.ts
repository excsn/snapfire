import type { Ctx } from "@snapfire/fsr/host";
import { books } from "@src/books";

export async function load({ session, query }: Ctx<"/">) {
  const shelf = query.shelf ?? "all";
  const rows = books.map((book) => {
    const read = session.progress[book.id] ?? 0;
    const place = session.done[book.id] ?? false ? "done" : read > 0 ? "reading" : "to-read";
    return { id: book.id, title: book.title, author: book.author, pages: book.pages, read, place };
  });
  return { shelf, rows: rows.filter((row) => shelf === "all" || row.place === shelf) };
}

export const meta = ({ data }: { data: { rows: unknown[] } }) => ({ title: `Shelf · ${data.rows.length} ${data.rows.length === 1 ? "book" : "books"}` });
