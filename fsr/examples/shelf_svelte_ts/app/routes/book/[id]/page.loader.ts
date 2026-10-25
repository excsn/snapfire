import type { Ctx } from "@snapfire/fsr/host";
import { fail } from "@snapfire/fsr/host";
import { books } from "@src/books";

export async function load({ params, session }: Ctx<"/book/{id}">) {
  const book = books.find((b) => b.id === params.id);
  if (book === undefined) fail("not_found", "there is no book with that id");
  return { book, read: session.progress[params.id] ?? 0, done: session.done[params.id] ?? false, current: session.reading === params.id };
}

export const meta = ({ data }: { data: { book: { title: string } } }) => ({ title: data.book.title });
