import type { Ctx } from "@snapfire/fsr";
import { books } from "@src/books";

export async function load({ session }: Ctx<"/stats">) {
  const rows = books.map((book) => ({ id: book.id, title: book.title, author: book.author, pages: book.pages, read: session.progress[book.id] ?? 0, done: session.done[book.id] ?? false }));
  return { rows };
}

export const meta = () => ({ title: "Shelf · Stats" });
