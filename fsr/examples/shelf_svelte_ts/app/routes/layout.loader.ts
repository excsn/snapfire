import type { Ctx } from "@snapfire/fsr/host";
import { books } from "@src/books";

export async function load({ session }: Ctx) {
  const done = books.filter((book) => session.done[book.id] ?? false).length;
  const reading = books.find((book) => book.id === session.reading);
  return { done, total: books.length, reading: reading?.title ?? "" };
}

export const store = ({ data }: { data: { done: number } }) => ({ "shelf/done": data.done });
