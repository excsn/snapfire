import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { Pick, Progress } from "@schemas/shelf";
import { books } from "@src/books";

export const progress = action(async ({ input, session }: ActionCtx<Progress>) => {
  const book = books.find((b) => b.id === input.book_id);
  if (book === undefined) fail("not_found", "there is no book with that id");
  if (input.page < 0) fail("invalid", "a page count is never negative");
  const page = Math.min(input.page, book.pages);
  const finished = page === book.pages;
  session.progress = { ...session.progress, [input.book_id]: page };
  session.done = { ...session.done, [input.book_id]: finished };
  const done = books.filter((b) => session.done[b.id] ?? false).length;
  return { page, finished, done };
});

export const pick = action(async ({ input, session }: ActionCtx<Pick>) => {
  const known = books.some((b) => b.id === input.book_id);
  if (!known) fail("not_found", "there is no book with that id");
  session.reading = input.book_id;
  return { reading: input.book_id };
});
